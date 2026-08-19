# CR-27 — the last schematized-and-unserved row gets a shape it can be served against, and params stop being ignored

**Status: DRAFT, unadjudicated, raised 2026-08-19.** Raised against `empyrean/contract/protocol.md` — the two
CRAM rows at `protocol.md:1049-1050`, catalogued in the commit that introduced that file and never served —
and lands as amendment **§11.17**. Three parts, one sitting. Two of them are a **specification pass on rows
that already exist**: no method is invented, no key is renamed, no shape is redesigned. The third is a
bus-wide rule about **request params**, and it is the only genuinely new normative content here.

It **adds one schema fragment and amends another**, taking the source `bus-protocol.schema.json` `methods`
object from **33 fragments to 34** — counted by parsing both revisions rather than by transcription, both
numbers reproduced in §6. Besides the two fragments it touches the schema in exactly three other places:
`emulator/write_memory`'s params gain one optional key, **every** `methods.<name>.params` object gains
`unevaluatedProperties: false`, and the top-level `description` is recounted. That is the whole of its
schema movement.

**Where this lives.** The CR is `oracle-next` branch `cr27-cram-params`, cut from `cram-serve-recon` at
`539009e` so it sits directly on top of its own recon. The contract edits are `empyrean` branch
`cram-params-amendment`, cut from `main` at `d72513c`, committed as a DRAFT and **not merged**.

**Sequencing, and one ordering hazard stated up front.** Contract-first, per the 2026-08-17 owner ruling
CR-21/22/23 cite: CR-27 is drafted and adjudicated before a line of handler is written, and the empyrean text
then merges **in the same window as the implementation**, on §11.15's sequencing precedent, so
`contract/protocol.md` on `main` is never ahead of the server nor behind it. The hazard is the entry number:
**§11.17 assumes §11.16 (CR-26) lands first.** CR-26's amendment is on the unmerged `profiler-amendment`
branch, and this branch is cut from `main`, which still ends at §11.15 — so on this branch §11.17 textually
follows §11.15 with a gap where §11.16 will be. The two branches also touch the same two places: the schema's
top-level fragment count (CR-26 recounts it to 36 for three profiler fragments; this CR recounts it to 34 for
one CRAM fragment — **the merged answer is 37**, and whichever merges second must recount rather than take
its own number) and the end of `protocol.md`. Both are mechanical conflicts at known coordinates, named here
rather than discovered at merge.

**This CR does not redesign anything.** The design is `docs/2026-08-19-cram-serve-recon.md`; the demand it
answers is transcribed from the requesting client's own probe in
`docs/2026-08-19-aurora-client-demand.md`. Where the recon left a choice a contract cannot leave open, or
where this CR reaches a different answer than the recon recommended, §7 says so out loud and names the
alternative it did not take — there are **seven** such places, and four of them are departures from the
recon's own recommendation.

**Serving these two methods needs no change request; amending their shapes does.** §8's prohibition runs the
other way — *"What the Oracle side must not do: invent new ops not in this spec"* (`protocol.md:1595`) — and
§9's standing note has the remaining fragments *"completed mechanically during conformance (§8 step 2) from
§6"* (`protocol.md:1618-1620`). Implementing a catalogued, schematized row is the compliant direction. The CR
exists because both rows need contract movement **before** either can be served conformantly, which is the
recon's §1.4 verdict and is re-derived in §1 below.

Every line anchor in this document was read in the file it names, in this session, at the revision named
beside it.

---

## 1. Why this is being raised

### 1.1 One row is the last of its kind, and the other was never schematized at all

`empyrean/contract/protocol.md:1049-1050`, verbatim as they stood before this CR:

```
| `emulator/read_cram` | `line`? (0–3) | `palette[]` |
| `emulator/write_cram` | `line`,`index`; `r`/`g`/`b`\|`raw` | `cramAddr`,`value` |
```

Both entered with **the commit that introduced `protocol.md`**, and `git log -S` over the schema returns
**exactly one commit** for `"emulator/write_cram"` — `a25bac1`, the same commit — so no change request has
touched that fragment since. `emulator/read_cram` has never had one at all.

Four facts follow, and together they are why a CR precedes a handler:

1. **`emulator/write_cram` is the one method in the schematized-and-unserved state today**, and §11.10's
   postscript records it in that same state at an older count. The reference server advertises **32**
   methods (parsed from `engine::METHODS`) and the schema holds **33** fragments; the one-element difference
   is `write_cram`. That element is the last living survivor of the mismatch that postscript logged — *"a schema count wrong on both ends (the
   schema holds 26 fragments and the reference server advertises 25; `write_cram` is schematized and not
   served)"* (`protocol.md:2352-2354`) — in a passage whose whole subject is *evidence that was wrong*.
2. **The reference suite already prints the gap and calls it a smell rather than a promise.**
   `schema_conformance.rs:162-168` computes the schematized-but-not-advertised set with the comment
   *"harmless in that direction — D4 makes the advertised list authoritative — but worth printing, because
   it is the shape of a method we might be missing."* It is a one-element list today.
3. **`write_cram`'s fragment predates every convention now governing it**, and §1.2 shows the three places
   that is load-bearing rather than cosmetic.
4. **`read_cram` cannot be served at all until it has a fragment.** §8 item 20 closes every result against
   its fragment at test time and the reference suite pins `UNCOVERED_METHODS` empty
   (`schema_conformance.rs:142`, `const UNCOVERED_METHODS: &[&str] = &[];`), so advertising a method with no
   fragment turns that test red. The contract-first ordering here is enforced by a gate, not requested by a
   convention — the same mechanism CR-26 leaned on.

### 1.2 The three defects in `write_cram`'s fragment — verified firsthand for this CR

The fragment, verbatim from `bus-protocol.schema.json:723-742` at `d72513c`:

```json
"emulator/write_cram": {
  "params": {
    "type": "object",
    "required": ["line", "index"],
    "properties": {
      "line": { "type": "integer", "minimum": 0, "maximum": 3 },
      "index": { "type": "integer", "minimum": 0, "maximum": 15 },
      "r": { "type": "integer", "minimum": 0, "maximum": 7 },
      "g": { "type": "integer", "minimum": 0, "maximum": 7 },
      "b": { "type": "integer", "minimum": 0, "maximum": 7 },
      "raw": { "type": "integer" }
    }
  },
  "result": {
    "allOf": [{ "$ref": "#/$defs/replyFields" }],
    "properties": {
      "cramAddr": { "$ref": "#/$defs/hex" },
      "value": { "$ref": "#/$defs/hex" }
    }
  }
}
```

| # | Defect | Why it is load-bearing |
|---|---|---|
| 1 | **The result declares no `required`.** | §8 item 20's closure is `unevaluatedProperties: false`, which catches a **surplus** key and says nothing about a **missing** one. A `write_cram` returning `{}` passed the gate — on the one row whose entire job is to confirm where the write landed. Verified by execution, not by reading: the empty reply is **accepted** by the pre-amendment fragment and **refused** by the amended one (§6's control). Every CR-era result fragment declares `required` — `read`'s at `:637`, `write_memory`'s at `:716`, `pixel_attribution`'s, `sprites`', `scanlines`'. |
| 2 | **`raw` is an unbounded integer.** | CRAM holds a 9-bit colour; the core masks with `0x0EEE` (`crates/oracle-core/src/vdp.rs:819`). The fragment permitted `raw: 70000`, which a server must then either refuse by a rule the schema did not carry, or silently mask — and silent mutation is the exact shape of the defect §2.5 exists to close. |
| 3 | **The `r`/`g`/`b`-versus-`raw` alternation is enforced nowhere.** | The §6 row spells it as an alternation and the fragment expressed none of it: both at once passed, a lone `r` passed, and neither-of-the-two passed. `write_memory`'s fragment is the house precedent for exactly this shape and spells it mechanically (`:699-704`: a `oneOf` over `bytes`/`value` plus a `dependentRequired` tying `value`↔`width`), with CR-24 setting the standard in as many words: the tie is *"enforced mechanically by an `if`/`then` in `result.allOf` rather than left to prose"* (`protocol.md:2547`). |

There is a fourth, smaller item: no `$comment` naming the row's §6 provenance — the convention §11.5's
result-key pass established and every fragment added or revised since carries — and no echo of `line`/`index`
on the reply, against the reason CR-20 stated for `read`: *"Echoed, so a reply is self-describing: `addr`
means nothing without the space it is in"* (`bus-protocol.schema.json:638`).

**The shape is sound and the fragment is not.** `line`/`index`/`r`/`g`/`b`/`raw` → `cramAddr`/`value` is the
right surface; it is the surface the legacy C++ server already serves and the surface the legacy MCP already
publishes a tool row for. Nothing is redesigned.

### 1.3 `read_cram`'s row does not say what it returns

One column wide — `line`? (0–3) → `palette[]` — and two questions it does not answer:

- **What an element of `palette[]` is.** A hex CRAM word (D9 category 1)? An `{r,g,b}` of stored 0–7
  components? A displayed 0–255 triple? Three different answers, and `pixel_attribution`'s own fragment
  already warns about confusing the last two: *"NOT the stored CRAM components — `emulator/write_cram`'s
  r/g/b (0-7) are the stored colour, this is the displayed one, and the two differ whenever `state` is not
  'normal'"* (`bus-protocol.schema.json:922`).
- **What omitting `line` returns.** 64 entries flat? Four arrays of 16?

**A finding this CR adds to the recon's:** the legacy C++ server answers both questions, in source, and this
CR deliberately does not adopt its answer. `OpReadCram` (`oracle/linux-port/gui/ControlSocket.cpp:2155-2209`)
returns `palette` as an array of **line objects** — `[{"line":N,"colors":[{index, cram_addr, raw, r, g, b,
r8, g8, b8} × 16]}]` — one such object with a `line` filter, four without. Three reasons that shape is not
taken, in §7 decision 3; the sharpest is the third: `r8`/`g8`/`b8` is a **bit-replication** expansion
(`:2197-2199`) while this server's single CRAM decode is linear `level × 255 / 7` (`vdp.rs:1535-1537`, tied
to the renderer by `cram_rgb_matches_cram_decoded`, `render.rs:1917-1926`). The two disagree at **three of
the eight levels** — computed for this CR: levels 2, 4 and 6 give 73/72, 146/145, 219/218. An 8-bit
expansion is a number two conformant servers would answer differently, and the catalog has never pinned the
ramp that would settle it (§10, `F-CRAM-RAMP`).

### 1.4 The demand, and who is asking

The requester is the suite's **editor**, and it matters *who* it is. Until 2026-08-19 every client on this
bus was the MCP — which D10 frames as the surface this protocol was extracted **from**. This is the first
client that learned the bus from `initialize` and the contract, with no shared history with the server, and
it did not ask from a design document: it connected, ran the handshake, read the advertised list, and called
things until they failed (demand doc §0).

**Nothing is blocked on us, and that is stated rather than glossed.** The demand's first item arrived as
*blocking* and **its own requester withdrew the blocking claim the same day**, with two anchors of their own,
after checking what a CRAM write survives on a running machine (demand doc §1, the supersession block at
`:45-67`). Their conclusion, in the demand doc's words: *"the live-palette demo is a
`write_memory`-to-RAM-source story, and `write_memory` already serves it. **Nothing of Aurora's is blocked on
us.**"* What remains is our own debt at our own priority — retiring the last schematized-and-unserved row —
and it is argued on that basis and no other.

**One item survived their correction untouched, and it is the sharpest.** `emulator/write_memory` accepts
unknown top-level params, ignores them, and reports success:

- `{symbol: "Player_1", offset: 2, value: …, width: 2}` → **succeeds**, writes `Player_1`+0.
- `{symbol: "Player_1", disp: 2, value: …, width: 2}` → **succeeds**, writes `Player_1`+0.
- `{symbol: "Player_1+2", …}` → **correctly refused** — the demand doc records this as `-32011`, which is
  a transcription slip corrected here: §5's table and the reference server both spell symbol-not-found
  **`-32013`** (`protocol.md:733`, `crates/oracle-aether/src/rpc.rs:46`), and `-32011` is not a code this
  contract defines. The finding is unaffected; only the number was wrong.

The mechanism is reproduced in source rather than taken on trust: `Engine::resolve_target`
(`crates/oracle-aether/src/engine.rs:986-1007`) reads `symbol`, else `addr`, and nothing else;
`write_memory` (`:1286-1300` onward) reads only `bytes`/`value`/`width` beyond that. **There is no key-set
check anywhere on this bus**, and no `params` fragment sets `additionalProperties: false` — checked across
all 33 by parse. Their warp writes `Player_1`+`$02` and +`$06`, so a client guessing a parameter name
corrupts a *different* player field and is handed a success reply naming an address it never asked to write.

Their sharpening is quoted because it reshaped the fix, and this CR would otherwise have built half of it:

> a `disp` param and reject-unknowns answer **different halves** — the footgun half is what bit them.

---

## 2. What exists today

**In the contract.** The two rows (`protocol.md:1049-1050`) in the *VRAM / CRAM / layers* table
(`:1044-1057`). §11.15 is the last amendment entry on `main` (`:2627`) and the file is 2724 lines, so an
entry appends at end of file. §2.4's conventions, §8 item 20's closure and §6's run-control state rule
(`:793-803`) are all in force and none of them was in force when these two rows were written.

**In the schema.** 33 fragments in `methods` (34 keys with the `$comment`), parsed. `emulator/write_cram` at
`:723-742`; `emulator/read_cram` **absent**. No `params` object anywhere sets `additionalProperties: false`
or `unevaluatedProperties: false`.

**In the reference server.** Neither method has a handler: `METHODS` holds 32 rows and none matches `cram`
(parsed from `engine.rs:155`ff). The read side needs nothing new — `Vdp::cram()` is already there
(`crates/oracle-core/src/vdp.rs:356-359`) and `Engine::read` already uses it for `space:"cram"`. The write
side needs a seam, because the guest path (`vdp.rs:818-826`) fires the watch capture at `:823`, which
`write_memory`'s own docstring forbids for a poke (`engine.rs:1283-1285`) and which §11.15 gave a second
reason to avoid — a captured CRAM write now carries the landing clock of the instruction that drove it, and
a poke has no instruction to take one from. That is a design question for the implementation slice; the only
part of it this CR carries is the **observable** consequence, which is normative and stated in §3.

**In the reference suite, one coverage gap the implementing agent inherits.**
`crates/oracle-aether/tests/write_memory.rs:109-117` exercises **seven** malformed payloads — both spellings,
neither, `width` with `bytes`, `value` without `width`, value over width, odd digit count, empty payload —
and **not one probes an unknown key**. There is no test that would go red today and none that pins the new
behaviour, so 27c's implementation is tests-first or it is nothing.

**On the demand side.** Their client's feature detection runs off the advertised list **live**, not planned:
*"`write_cram` will light up in their client automatically the day we advertise it, with no coordination
needed"*, and the legacy MCP behaves the same way for the same reason — `served_methods()`
(`oracle/linux-port/mcp/oracle_mcp.py:856-876`) returns the server's advertised set and `list_tools()`
(`:879-891`) skips any tool whose method is absent, at the `continue` on `:890-891`. The `emulator_read_cram` (`:801-814`) and
`emulator_write_cram` (`:815-847`) tool rows **already exist** and are hidden only because oracle-next does
not advertise the methods. The demand doc draws the conclusion this CR adopts as its acceptance property:
*"That is D4 paying off, and it is also a warning: advertising a method is shipping it."*

---

## 3. The proposed §6 text (verbatim, as it landed on the branch)

Four edits, all in §6. **Every block below was extracted mechanically from `cram-params-amendment` at
`39628e2` and never retyped** — CR-25's D-M1 lesson, which CR-26 inherited: a block labelled verbatim stops
being verbatim the moment its source is edited.

**(a) The two rows** — `contract/protocol.md:1113-1114` on the branch:

```markdown
| `emulator/read_cram` | `line`? (0–3) | `line`?, `palette[]{line,index,cramAddr,raw,r,g,b}` |
| `emulator/write_cram` | `line` (0–3), `index` (0–15); (`r`+`g`+`b`, 0–7 each) \| `raw` (≤ `$0EEE`) | `line`,`index`,`cramAddr`,`value` |
```

**(b) A normative blockquote for the pair**, inserted directly after the `emulator/scanlines` blockquote and
before `### object / player decoders` — `contract/protocol.md:1283-1345`:

```markdown
> **`emulator/read_cram` / `emulator/write_cram` — the palette, read and poked** *(specified
> 2026-08-19, §11.17)*. Both rows were catalogued from the legacy socket in the first draft of this
> document and neither had ever been served; `write_cram`'s fragment predated every convention now
> governing it and `read_cram` had none at all, which is why this is a specification pass and not a new
> family. A CRAM entry is addressed by a `line` (0–3) and an `index` (0–15) in both directions, and the
> pair a read hands out is the pair a write takes back. Seven behaviours are normative.
>
> - **`read_cram` answers the whole palette or one line of it, in one shape.** With `line` the reply
>   carries that line's 16 entries and **echoes `line`**; without it, all 64, and `line` is **absent** —
>   its presence is what tells a client which of the two answers arrived, and the fragment ties the echo
>   to the array's length in **both** directions (`emulator/read`'s region-present-iff-`bus` precedent).
>   Entries are line-ascending, then index-ascending, and contiguous. `palette` is bounded at 4×16 **by
>   the video hardware**, so it carries neither a truncation flag nor a cursor (§2.4 clause (d):
>   structural bound → neither, `pixel_attribution`'s `candidates` being the exemplar), and its length is
>   fixed by the request — a partial palette is not expressible.
>
> - **An entry is the STORED colour, never the displayed one.** `raw` is the stored 9-bit word and
>   `r`/`g`/`b` its 3-bit components (0–7) — `write_cram`'s own spelling, so an entry hands straight
>   back. The colour a dot is actually shown in is `emulator/pixel_attribution`'s `rgb`, which runs the
>   same components through an intensity ramp at the resolved shadow/highlight state and **differs
>   whenever that state is not `normal`**. No 8-bit expansion appears on this row: this catalog has never
>   pinned a ramp, the two servers that compute one disagree at three of the eight levels, and a number
>   two conformant servers answer differently is worse here than an absent one. Pinning it belongs to the
>   methods that own the displayed colour.
>
> - **`cramAddr` rides on every entry and on the write's reply, and it is the join key.** It is
>   `(line × 16 + index) × 2`, and so derivable — carried anyway because three other surfaces name a CRAM
>   byte address and none of them names `(line, index)`: `pixel_attribution.cramAddr`, the `space`+`addr`
>   pair a `cram` watch hit reports, and `emulator/read` with `space: "cram"`. Making a client recompute
>   the key it needs to join four instruments is the recompute `read`'s echo rule exists to prevent.
>   `pixel_attribution`'s `cramIndex` is deliberately **not** carried: it is `line × 16 + index`, and that
>   method emits it only because it has no `(line, index)` pair to give — `emulator/sprites`' omitted
>   per-entry `satAddr` is the same rule reaching the opposite answer for the opposite reason.
>
> - **`write_cram` takes exactly one colour spelling.** All three of `r`/`g`/`b`, or `raw` — never both,
>   never a partial triple, never neither. Each of those four is `-32602`, and the fragment enforces all
>   four mechanically rather than leaving them to this paragraph (`write_memory`'s `bytes`-XOR-`value`+
>   `width` shape). A `raw` carrying bits outside the chip's `$0EEE` mask is **`-32602` — refused, never
>   masked**: the reply's whole job is to say where the write landed, and a reply reporting a value the
>   caller did not send is the silent mutation this bus refuses everywhere else. `line` or `index` out of
>   range is `-32602`, refused, never clipped.
>
> - **The reply echoes `line` and `index` beside `cramAddr` and `value`, and all four are REQUIRED.**
>   `value` is the word **actually stored**. Before this amendment the fragment required nothing at all,
>   and §8 item 20's closure catches a surplus key while saying nothing about a missing one — so a server
>   answering `{}` would have passed every gate this catalog has, on the one row whose entire purpose is
>   confirming where a write went.
>
> - **`write_cram` requires a paused machine; `read_cram` is a pure read.** The write is named in §6's
>   run-control state rule (`-32005`, `data.reason = "machineRunning"`). The read is not, and a server
>   MUST NOT refuse it on a free-running machine — exactly as `read`, `sprites`, `pixel_attribution` and
>   `scanlines` are not refused; D11's stamp is the whole answer to a torn palette sample.
>
> - **A poke is a debugger access, and two standing properties follow.** It is **never offered to the
>   watch surface**: no `cram` watch matches it and `watchpoint_hits.seen` does not move, because a hit's
>   `pc` names the instruction that drove the access and a poke has none to name (`write_memory`'s rule,
>   unchanged) — and since §11.15 a captured CRAM write also carries the landing clock its instruction
>   supplies, which an instruction-less write cannot. And it does **not** repaint a frame already drawn:
>   `emulator/scanlines` goes on reporting the retained frame's colours until the machine advances, while
>   `pixel_attribution`, which re-derives from live state, changes at once. That is the same
>   retained-versus-re-derived split §11.3 and §11.14 already document. Both are **standing properties**
>   of the pair, stated here once rather than as a `caveat` on every reply — §2.4's advisory, and
>   §11.15's own reason for putting a permanent property in this document instead.
```

**(c) The run-control state rule gains `write_cram`** — `contract/protocol.md:840-854`, the changed list and
the added sentences shown whole so the reason travels with the name:

```markdown
> **Run-control state rule.** `run_to`, `run_to_scanline`, `run_frames`, `step*`, `press`, `play_input`,
> `reload_rom`, `write_memory` and `write_cram` require a **paused** machine. Called while it is free-running they MUST fail with
> `-32005` (`data.reason = "machineRunning"`), never pause implicitly (§5). *Why `press` and
> `reload_rom` are named alongside the run-shaped ops:* they mutate the timeline just as surely —
> `press` holds buttons across a bounded run of frames, `reload_rom` swaps the cartridge out from
> under whatever is executing — so a caller who issues either against a free-running machine cannot
> say afterwards what it acted on, and §5's ban on resolving a wrong-state case implicitly applies to
> them word for word. Leaving them unnamed would let one server refuse and another accept, both
> conforming. `write_memory` is named for `press`'s reason — a poke mid-free-run mutates the timeline
> just as surely, and leaving it unnamed would let one server refuse and another accept, both
> conforming. `write_cram` *(named 2026-08-19, §11.17)* is named for `write_memory`'s reason and for a
> second one the client who asked for the method supplied: on a running machine a game that composes its
> palette every frame overwrites a direct CRAM write inside the frame it lands in, so the free-running
> call is not a weaker answer but a vanishing one. The paused machine is where the method does its work,
> which is a better argument for the gate than symmetry.
```

**(d) `emulator/write_memory` gains `disp`** — the row at `contract/protocol.md:900`:

```markdown
| `emulator/write_memory` ← `write` | `addr`\|`symbol`, `disp`? (≥0, `symbol` only); `bytes`\|(`value`+`width` 1\|2\|4); payload ≤ `limits.maxWriteLen` bytes | `addr`, `len` |
```

and a paragraph appended to that method's existing blockquote — `contract/protocol.md:963-974`:

```markdown
> **`disp`** *(added 2026-08-19, §11.17)* is an optional non-negative byte displacement added to the
> address `symbol` resolves to. It is valid **only** with `symbol`: with `addr` it is arithmetic the
> caller has already done, so `{addr, disp}` is `-32602`, enforced by the fragment rather than by prose.
> It mirrors the pair a read reply already hands back — `{symbol, symbolDisp}` out, `{symbol, disp}` in —
> which is D7's round trip made literal, and it is non-negative for the same reason `symbolDisp` is: that
> field is a displacement from the **nearest preceding** symbol and cannot be negative. The displaced
> address must still land in the window above (`-32004` otherwise), and the reply's `addr` is the
> resolved, displaced one. *Why the ergonomic half is shipped beside §2.5's safety half:* the client that
> found the footgun wanted `Player_1`+2 as a **symbol-relative** request and reached for the nearest
> plausible parameter name. Rejecting unknown params alone would leave them computing hex addresses in an
> editor, against D7's whole point; adding `disp` alone would leave the next client's `offset:` guess
> exactly as silently wrong.
```

---

## 4. The proposed §2.5 and §8 item 22 (verbatim)

This is the CR's one new rule, and it is the item most likely to attract a ruling. It is bundled with two
mechanical fragment fixes deliberately, on the recon's §2 recommendation (A): they are one sitting's work,
they all come from one client's report, and bundling gets 27c read.

**§2.5**, inserted after §2.4 and before §3 — `contract/protocol.md:561-606`:

```markdown
### 2.5 Request params are closed

*(Added 2026-08-19, §11.17.)* §2.4 governs what a server may put in a **result**. This governs what a
client may put in **params**, and it is §8 item 20 with the subjects reversed.

**A server MUST refuse a request carrying a top-level `params` key that the method's schema fragment does
not declare.** The code is `-32602`. The `message` names the offending key **and** lists the keys the
method accepts, so the refusal is also the fix — the shape §5 already asks for (*"Refuse, name the reason,
and name the fix"*). `error.data.unknownParams` carries the offending keys as an array of strings, because
a client acting on *which* key was rejected needs a typed field rather than prose (§2.4 rule 3: any
consequence a client must act on needs its own typed key). The refusal **precedes any effect**: a write
refused for an unknown param has written nothing.

**The closure is at the top level of `params`** — item 20's own scope, for its reason — and the schema
carries it mechanically: every `methods.<name>.params` object in
[`schema/bus-protocol.schema.json`](schema/bus-protocol.schema.json) declares
`unevaluatedProperties: false`. Objects nested inside a params payload are closed only where their own
subschema closes them.

**`initialize` is exempt, deliberately.** Its params — and `clientCapabilities` inside them — are the one
place on this bus where a client describes *itself*, and D4 makes that exchange the precondition for
everything else. A client built against a later revision must still be able to hand its capabilities to an
earlier server and be understood as far as that server goes; closing the handshake would make a
version-skewed negotiation fail at the single step whose job is to survive skew. D5's *"old clients ignore
unknown flags"* keeps its full meaning there.

> **Why this closure is published when item 20's is not.** Item 20 put the *result* closure in the
> conformance harness and deliberately **not** in the published artifact: *"Closure in the published
> artifact would weaponise stale schemas against conformant servers — D5's preserved-defect argument
> inverted"*, reconciled as *"Closure binds servers; additivity protects clients."* Reverse the subjects
> and the reconciliation reverses with them. On a **result** the server writes and the client validates, so
> a published closure lets a client's month-old copy reject a server that did nothing wrong. On **params**
> the client writes and the **server** validates — against the revision it implements, at a moment when an
> unknown key means *"I guessed a parameter name"* rather than *"the contract moved on."* The party
> validating is the party publishing what it supports, so the stale-artifact hazard that kept item 20 out
> of this file does not exist in this direction. Additivity protected the client and still does; it never
> entitled a client to send keys the server never registered.
>
> **The cost, stated plainly, because it is real.** An optional param added in a later amendment stops
> being invisible to older servers: they refuse it, by name. That is the trade this rule makes on purpose —
> a named refusal a client can branch on, in place of a silent misreading it cannot detect — and it is the
> reason the refusal must name the key rather than say "invalid params". The case that prompted the rule is
> the measured one: a client wanting to write `Player_1`+2 reached for `offset:`, then for `disp:`, and was
> told **OK** both times while the server wrote `Player_1`+0 and answered with an address the client had
> never asked for. Guessing a parameter name was never a discovery mechanism; it only resembled one while
> it silently succeeded.
```

**§8 item 22** — `contract/protocol.md:1670-1681`:

```markdown
Added by the 2026-08-19 amendment (§11.17):

22. **Refuse unknown top-level params — in the server** (§2.5). A request carrying a `params` key its
    method's fragment does not declare is `-32602`, with the offending key named in `message`, the
    accepted keys listed beside it, and `error.data.unknownParams` carrying the offending keys as an
    array. The refusal precedes any effect. This is item 20's rule with the subjects reversed, and
    unlike item 20 the closure **is** in the published schema — every `methods.<name>.params` declares
    `unevaluatedProperties: false` — because on the request path the validating party is the publishing
    party, so there is no stale-artifact hazard to protect a conformant peer from. `initialize` is
    **exempt**: closing the handshake would make a version-skewed negotiation fail at the one step whose
    job is to survive skew. Listed here rather than left to §2.5 because the defect it closes was
    invisible to the reference server's whole suite and was found by a client's first probe.
```

---

## 5. The §11.17 entry (verbatim)

Appended at end of `protocol.md`, which is 2724 lines on `main` at `d72513c`. §3's and §4's insertions push
everything below them down by **141** lines — counted mechanically against the amended branch (§11.15's own
heading moves `:2627` → `:2768`), not adjusted by hand — which is why the entry begins at `:2867` in a file
that is 2977 lines there.

**Byte-identical to `contract/protocol.md:2867-2977` on `cram-params-amendment` at `39628e2`.**

```markdown
### 11.17 — 2026-08-19: the last schematized-and-unserved row, and the params dropped on the floor

**CR-27**, raised in `oracle-next/docs/2026-08-19-cr27-cram-params.md` off the design in
`oracle-next/docs/2026-08-19-cram-serve-recon.md` and the demand transcribed from the requesting client's
own probe in `oracle-next/docs/2026-08-19-aurora-client-demand.md`. It appends after **§11.16** (CR-26).
The requester is the suite's **editor** — the first client to learn this bus from `initialize` and this
document rather than from shared history with the server, and the first that is not the MCP, of which D10
says in as many words that it *"becomes one client of Aether, not the definition of it."* This is the
amendment where that stops being an aspiration, and D10 named the occasion too: *"The TypeScript editor
client follows when the palette/warp workflows land."* Those are the two workflows in this entry — the
palette is 27a and 27b, and the warp is what 27c's `disp` was reached for. What the editor tripped over is
therefore evidence about this catalog's legibility rather than about one client's habits, and it is the
reason a small amendment carries a bus-wide rule.

Three parts, and none of them a new family: **27a** amends a fragment older than every convention that now
governs it, **27b** writes the first fragment for a row that never had one, **27c** answers the one live
defect the requester found — with the ergonomic half and the safety half shipped together, because they are
different halves and only the second is a footgun.

| Item | The defect | What this amendment changed |
|---|---|---|
| **CR-27a — `emulator/write_cram`'s fragment** | The fragment is from the **original Phase-1 contract commit** and no change request had touched it since, so it predates §11.5's result-key pass, §2.4's conventions and §8 item 20's closure — and it showed. Its `result` declared **no `required`**, and item 20's `unevaluatedProperties: false` catches a *surplus* key while saying nothing about a *missing* one: a server answering `{}` passed every gate this catalog has, on the one row whose entire job is to report where a write landed. `raw` was an **unbounded integer** against a chip that holds nine bits, so the fragment permitted a value the server must then either refuse by a rule the schema did not carry or silently mask. And the `r`/`g`/`b`-versus-`raw` alternation the §6 row states as an alternation was enforced **nowhere**: both at once passed, and so did a lone `r`. | The `result` requires **all four** keys — `line` and `index` **echoed** beside `cramAddr` and `value`, on the `read` echo precedent, because `cramAddr` alone makes a client recompute its own request to confirm it. `raw` is bounded to `0x0EEE` in the schema, with the exact-mask rule in prose and the `$comment`: bits outside the mask are **`-32602`, refused and never masked**, the coarser mechanical bound standing in the same relation to it as `write_memory`'s `value` bound does to must-fit-`width`. The alternation becomes mechanical — a `oneOf` over the triple and `raw` plus a `dependentRequired` tying `r`/`g`/`b` to each other — which refuses all four bad spellings (both, neither, partial triple, partial triple beside `raw`) without a sentence being consulted. The fragment gains its `$comment`, and declares `caveat` **absent** on the `sprites`/`write_memory` precedent. No key is renamed and no shape is redesigned: `line`/`index`/`r`/`g`/`b`/`raw` → `cramAddr`/`value` was the right surface and remains it. |
| **CR-27b — `emulator/read_cram`'s first fragment** | The row was catalogued one column wide — `line`? (0–3) → `palette[]` — and **had no schema fragment at all**, which the requester did not know: they read both methods as schematized, and the practical consequence is that this half cost a fragment and the other did not. The row did not say what an element of `palette[]` **is** — a hex CRAM word, an `{r,g,b}` of stored components, or a displayed 0–255 triple are three different answers, and `pixel_attribution`'s fragment already warns about confusing the last two — nor what omitting `line` returns. | A fragment, **33 → 34**, and a row that answers both questions. An element is `{line, index, cramAddr, raw, r, g, b}`: the **stored** colour, in `write_cram`'s own spelling, so a read entry hands straight back — never the displayed one, which is `pixel_attribution`'s `rgb` and differs whenever shadow/highlight is not `normal`. **No 8-bit expansion is emitted**, deliberately: this catalog has never pinned an intensity ramp and the two servers that compute one disagree at three of the eight levels. Omitting `line` returns all 64 entries with `line` **absent** from the reply; giving it returns 16 with `line` **echoed**; the echo and the length are tied in both directions by the fragment (`read`'s region-present-iff-`bus` precedent). `palette` is **structurally** bounded and therefore takes neither a truncation flag nor a cursor (§2.4 clause (d)). A pure read, ungated. |
| **CR-27c — the params policy** | `emulator/write_memory` accepted `{symbol: "Player_1", offset: 2, …}` and `{symbol: "Player_1", disp: 2, …}`, **succeeded both times**, wrote `Player_1`+0 both times, and answered with an address the caller had not asked for; `{symbol: "Player_1+2"}` was correctly refused. Unknown top-level params were dropped on the floor bus-wide — no `params` fragment set `additionalProperties: false`, and no handler on the bus checked a key set — so the published schema *permitted* the surplus on every method and a server that refused one would have been stricter than the artifact D14 makes the wire authority. The requester's own framing is what reshaped the fix: a `disp` param and reject-unknowns answer **different halves**, and the footgun half is what bit them. | **§2.5** (new) makes the rejection normative bus-wide: an undeclared top-level `params` key is `-32602`, naming the offending key **and** the accepted set in `message`, with `error.data.unknownParams` carrying the keys as a typed array, and refused **before any effect**. **§8 item 22** puts it on the conformance checklist. The closure is spelled mechanically in the **published** schema — every `methods.<name>.params` gains `unevaluatedProperties: false` — which is where this differs from item 20, and the reasoning is item 20's own with the subjects reversed. **`initialize` is exempt**, deliberately. And the ergonomic half ships beside it: `emulator/write_memory` gains an optional **`disp`**, non-negative, valid only with `symbol`, mirroring the `{symbol, symbolDisp}` pair a read reply already returns — D7's round trip made literal — with `{addr, disp}` refused mechanically. |

**★ Nothing was blocked on this bus, and the amendment says so.** The demand's first item arrived as
*blocking*, and **its own requester withdrew that claim the same day** after checking what a CRAM write
survives on a running machine: their live-palette demo turned out to be a `write_memory`-to-RAM-source
story, which this bus already serves. What is left is **our** debt at **our** priority — `write_cram` is the
one method in the schematized-and-unserved state today, and §11.10's postscript records it in that same
state at an older count, so it is the last living survivor of the mismatch that entry logged — and it is
worth saying plainly that a register which quietly edits its own entries stops being evidence. The
withdrawal is recorded beside the original ask rather than in place of it, and the shape asked for is the
shape specified here; only the priority and the reason moved.

**★ Why the params closure is published when §8 item 20's is not.** This is the amendment's one genuinely
new rule, and it is decided by a symmetry the catalog had already reasoned out in the other direction. Item
20 kept the *result* closure out of the published artifact because *"Closure in the published artifact would
weaponise stale schemas against conformant servers"*, reconciled as *"Closure binds servers; additivity
protects clients."* On a result the server writes and the client validates, so a published closure lets a
month-old copy reject a server that did nothing wrong. On params the client writes and the **server**
validates — against the revision it implements, at a moment when an unknown key means *"I guessed a
parameter name"*, not *"the contract moved on"* — and the party validating is the party publishing what it
supports, so the hazard that kept item 20 out of the schema does not exist in this direction. The cost is
real and is stated in §2.5 rather than buried: an optional param added later stops being silently ignored by
older servers, and becomes a **named** refusal instead. That is the trade taken on purpose. `initialize` is
the one carve-out, because a version-skewed client must still be able to describe itself to an older server,
and closing the handshake would break negotiation at the step whose whole job is to survive skew.

**★ The pause gate is demand-side confirmation, not symmetry.** `write_cram` joins the run-control state
rule's named list, and the argument is better than "it looks like `write_memory`": the requester established
from their own engine that a direct CRAM write on a **running** machine is overwritten inside the frame it
lands in by a palette pipeline composed once per frame, so the free-running call is not a weaker answer but a
vanishing one. Where the method earns its keep is the **paused** machine — inspect a colour, change it, see
it on glass with nothing stepping on it — which is exactly the shape the gate gives. `read_cram` is
**ungated** on the `read` / `sprites` / `pixel_attribution` / `scanlines` precedent, which is also what the
requester asked for.

**★ What does not change.** The six param names and the two reply keys `write_cram` was catalogued with: no
rename, no redesign, and the legacy MCP's two tool rows keep their existing parameter schemas, which already
match this row. The count of methods a server advertises is a server's business (D4), and this document
still maintains none. `read_cram`'s `line` param, its range and its optionality. `pixel_attribution`'s
`rgb`, `cramIndex` and `cramAddr`, and `emulator/read`'s `space: "cram"`, which remain the other ways to ask
about a palette entry and are unmoved. Every result closure in §8 item 20, which is untouched by §2.5 —
results stay open in the published artifact for D5's reason. And **§11.10's postscript sentence recording
the count mismatch stays as written**: an amendment log is a record, not live text, which is the treatment
§11.14 gave the same class of sentence.

**★ What this amendment does not carry.** Four items, each named rather than folded in. A **whole-line batch
`write_cram`** was offered by the requester and **withdrawn by them** the same day with a reason — the drag
loop it was sized for is not a CRAM loop — so it is recorded as withdrawn, not deferred, and carries no
follow-up id: a deferral is a debt this catalog owes and a withdrawal is not. **`F-PALETTE-DRAG-PACE`** is
its registered-but-not-requested neighbour: a `write_memory`-driven palette drag at 30 Hz is 30
pause/write/resume cycles per second and **60 `stopped`/`resumed` events per second** to every subscriber;
the requester raised it themselves, asked for nothing, and plans to throttle and measure first, so the two
shapes they named — relaxing the pause gate for small bounded writes, or an apply-at-next-frame-boundary
write — are revived by numbers or not at all. **`disp` is added to `write_memory` only**, not to every method
taking `addr`/`symbol`: widening it later is additive under D5 and belongs to the client that asks. And the
**8-bit expansion** of a stored colour is left unspecified here on purpose, because pinning an intensity ramp
is the business of the rows that answer for the displayed colour.

*Adoption condition, per §11.6 / §11.8 / §11.10 / §11.11 / §11.14 / §11.15 / §11.16, in CR-24's two-part
structure — suite gates executable in the reference repo, plus a demand-side acceptance protocol:*
registered when **(1)** a conformant reply passes **both** fragments **closed** (`unevaluatedProperties:
false` at test time, §8 item 20), happy path plus **one refusal per catalogued bound** — `write_cram` on a
free-running machine → `-32005` (`machineRunning`), `line` or `index` out of range → `-32602`, the triple
and `raw` together → `-32602`, a `raw` with bits outside `$0EEE` → `-32602` **refused and not masked**, and
`read_cram` with `line` outside 0–3 → `-32602` refused and not clipped; **(2)** the shape rules the
fragments carry mechanically are each proven by a message that **fails** them — a `write_cram` result
missing `value`, a partial `r`/`g`/`b` triple, and a `read_cram` reply whose `line` echo and `palette`
length disagree in either direction — since a closure nobody has watched reject is a closure nobody has
tested; **(3)** the unknown-param rejection is proven by a probe **that passed silently before it**:
`{symbol, offset, value, width}` → `-32602` naming `offset`, asserted on both the typed
`error.data.unknownParams` and the message text, with the sentinel byte at the target unchanged — the
reference suite's existing seven-payload refusal loop probes **no** unknown key today, so this behaviour has
no test that would go red and none that pins it, and tests-first is the only honest order; **(4)** `disp`
round-trips literally — a `read` reply's `{symbol, symbolDisp}` handed back as `{symbol, disp}` writes the
byte the read named, `{addr, disp}` is `-32602`, and a `disp` past the work-RAM window is `-32004`;
**(5)** the **watch surface stays silent** on a `write_cram` — a `cram` watch armed, the entry poked, zero
hits and `seen` unmoved — which is the direct pin of the standing property above and the one gate that would
catch a later simplification into the guest write path; and **(6)** the two methods read back through each
other and through a third instrument — `write_cram`'s `cramAddr`/`value` agree with the same entry from
`read_cram` **and** with `emulator/read` at `space: "cram"` — against a CRAM state the test itself
established, since an assertion that still passes when the handler returns a fixed palette is vacuous.
Clauses 1–6 are executable in the reference repo. The demand-side protocol is the requester's own offer,
taken: **they point their probe at the branch before it merges**, and the property under test is the one
they named — *"advertising a method is shipping it"*. Their client's feature detection runs off the
advertised list **live**, so the acceptance is that both methods appear in `initialize`'s `methods` and light
up in their editor with **no coordination**, that the probe which succeeded silently now refuses by name, and
that a symbol-relative write lands where they meant it to. That protocol is not a suite gate — the client is
theirs — exactly as §11.14's A1/A2 sweep and §11.16's parity corpus are not.
```

---

## 6. The schema — one fragment written, one amended, one param added, every params object closed

**Counts, parsed rather than transcribed.** Before this CR the source
`contract/schema/bus-protocol.schema.json` `methods` object held **34 top-level keys = 33 fragments + one
`$comment`**; after it holds **35 = 34 + `$comment`**. The one added key is `emulator/read_cram`. Both
numbers come from `json.load(…)` over the two revisions — the accounting CR-25 and CR-26 both used, and for
the reason CR-26 gave: a transcribed count is a third answer that has to be kept equal to two others by hand.

**`emulator/read_cram` — the new fragment**, verbatim from
`contract/schema/bus-protocol.schema.json:737-779` on the branch:

```json
    "emulator/read_cram": {
      "$comment": "protocol.md §6 (VRAM / CRAM / layers), specified 2026-08-19 by §11.17 (CR-27). This row's FIRST fragment: it was catalogued from the legacy socket and never schematized, which is why it could not be served — §8 item 20 closes results against fragments, so a method without one would ship a result nobody had checked. The whole palette or one line of it: `line` omitted returns all 64 entries and is ABSENT from the reply, `line` given returns that line's 16 and is ECHOED, and the echo<->length tie is enforced by the result's if/then in both directions (the `read` region precedent). A pure read: a server MUST NOT refuse it on a free-running machine, exactly as read, sprites, pixel_attribution and scanlines. `palette` is STRUCTURALLY bounded (4x16 by the video hardware), so it takes neither total/returned/truncated nor a cursor (§2.4 clause (d), pixel_attribution's `candidates` precedent). Entries carry the STORED colour — r/g/b are emulator/write_cram's 0-7 components and `raw` the stored word — never a displayed one: pixel_attribution.rgb is the displayed colour and the two differ whenever `state` is not 'normal'. No 8-bit expansion is emitted: this catalog has never pinned an intensity ramp, and the two servers that compute one disagree (the legacy socket bit-replicates, the reference server runs level*255/7 — one apart at levels 2, 4 and 6), so an unpinned number two conformant servers answer differently is worse here than an absent one. caveat is declared ABSENT, not omitted by accident (the sprites precedent): reading live CRAM has no weaker-answer condition to warn about.",
      "params": {
        "type": "object",
        "unevaluatedProperties": false,
        "properties": {
          "line": { "type": "integer", "minimum": 0, "maximum": 3, "description": "Palette line to read. Omitted: all four lines, 64 entries. Outside 0-3 is -32602 — refused, never clipped. An index, so a JSON number (D9 category 2)." }
        }
      },
      "result": {
        "allOf": [
          { "$ref": "#/$defs/replyFields" },
          {
            "$comment": "The echo and the length are tied in BOTH directions: 16 entries iff `line` is echoed, 64 iff it is not. The `read` fragment's region-present-iff-bus if/then is the precedent, and the reason is the same — a client must be able to tell which of the two answers it is holding without counting an array.",
            "if": { "required": ["line"] },
            "then": { "properties": { "palette": { "minItems": 16, "maxItems": 16 } } },
            "else": { "properties": { "palette": { "minItems": 64, "maxItems": 64 } } }
          }
        ],
        "required": ["palette"],
        "properties": {
          "line": { "type": "integer", "minimum": 0, "maximum": 3, "description": "Echoed IFF the param was given. Its presence is what says which of the two answers this is." },
          "palette": {
            "type": "array",
            "description": "The requested entries, line-ascending then index-ascending, contiguous. Structurally bounded, so no total/returned/truncated and no cursor (§2.4 clause (d)); the length is fixed by the request and a partial list is not expressible.",
            "items": {
              "type": "object",
              "required": ["line", "index", "cramAddr", "raw", "r", "g", "b"],
              "additionalProperties": false,
              "properties": {
                "line": { "type": "integer", "minimum": 0, "maximum": 3, "description": "Palette line. Carried on every entry, the single-line answer included, so one entry on its own addresses the same cell through emulator/write_cram." },
                "index": { "type": "integer", "minimum": 0, "maximum": 15, "description": "Index within the line — emulator/write_cram's `index`, so the (line,index) pair hands straight back." },
                "cramAddr": { "$ref": "#/$defs/hex", "$comment": "CRAM byte address of the entry, (line*16 + index)*2. Derivable, and carried anyway because it is the JOIN key three other surfaces speak and (line,index) is not: pixel_attribution.cramAddr, the space+addr pair a cram watch hit reports, and emulator/read with space 'cram'. pixel_attribution's cramIndex is NOT carried — it is line*16 + index, and that method emits it only because it has no (line,index) pair to give (emulator/sprites' omitted per-entry satAddr is the same rule reaching the opposite answer for the opposite reason)." },
                "raw": { "$ref": "#/$defs/hex", "description": "The stored CRAM word, masked to the 9-bit colour (0x0EEE, ---- BBB- GGG- RRR-). D9 category 1." },
                "r": { "type": "integer", "minimum": 0, "maximum": 7, "description": "Stored red component, 3-bit — emulator/write_cram's `r`. A component, so a JSON number (D9 category 2). NOT the displayed colour; see pixel_attribution.rgb." },
                "g": { "type": "integer", "minimum": 0, "maximum": 7, "description": "Stored green component, 3-bit." },
                "b": { "type": "integer", "minimum": 0, "maximum": 7, "description": "Stored blue component, 3-bit." }
              }
            }
          }
        }
      }
    },
```

**`emulator/write_cram` — the amended fragment**, verbatim from `:780-807`:

```json
    "emulator/write_cram": {
      "$comment": "protocol.md §6 (VRAM / CRAM / layers), amended 2026-08-19 by §11.17 (CR-27). This fragment came from the ORIGINAL Phase-1 contract commit and no CR had touched it since: it predates §11.5's result-key pass, §2.4's conventions and §8 item 20's closure, and it showed. Three defects are now mechanical. (1) The result declared no `required`, and item 20's closure catches a SURPLUS key while saying nothing about a MISSING one — so a reply of {} passed the gate, on a row whose whole job is to confirm where the write landed. (2) `raw` was an unbounded integer against a 9-bit chip. (3) The r/g/b-vs-raw alternation the §6 row states was enforced nowhere: both at once passed, and so did a lone `r`. The alternation is now a oneOf plus a dependentRequired over the triple (emulator/write_memory's bytes-XOR-value+width precedent), and the result requires all four keys, `line`/`index` echoed so a reply is self-describing — cramAddr alone makes a client recompute its own request to confirm it (the `read` echo precedent). Requires a paused machine per the §6 run-control state rule (-32005 machineRunning). Prose-only, because JSON Schema's integer keywords cannot express it: a `raw` carrying bits outside the 0x0EEE mask is -32602, REFUSED and never masked — the coarser bound below is the mechanical half, exactly as write_memory's `value` is bounded 0..2^32-1 with must-fit-`width` left to the server. caveat is declared ABSENT, not omitted by accident (the sprites and write_memory precedent): the poke's two standing properties — never offered to the watch surface, and no repaint of an already-drawn frame — are stated once in §6 rather than repeated on every reply (§2.4's advisory against the constant caveat, and §11.15's own reason for stating a standing property in the document).",
      "params": {
        "type": "object",
        "unevaluatedProperties": false,
        "required": ["line", "index"],
        "oneOf": [{ "required": ["r", "g", "b"] }, { "required": ["raw"] }],
        "dependentRequired": { "r": ["g", "b"], "g": ["r", "b"], "b": ["r", "g"] },
        "properties": {
          "line": { "type": "integer", "minimum": 0, "maximum": 3, "description": "Palette line. Outside 0-3 is -32602 — refused, never clipped. An index, so a JSON number (D9 category 2)." },
          "index": { "type": "integer", "minimum": 0, "maximum": 15, "description": "Index within the line. Outside 0-15 is -32602 — refused, never clipped." },
          "r": { "type": "integer", "minimum": 0, "maximum": 7, "description": "Stored red component, 3-bit. Travels with g and b — a partial triple is -32602 — and the triple and `raw` are alternatives, so passing both is -32602 too." },
          "g": { "type": "integer", "minimum": 0, "maximum": 7, "description": "Stored green component, 3-bit. Travels with r and b." },
          "b": { "type": "integer", "minimum": 0, "maximum": 7, "description": "Stored blue component, 3-bit. Travels with r and g." },
          "raw": { "type": "integer", "minimum": 0, "maximum": 3822, "description": "The whole stored word as a number: at most 0x0EEE (---- BBB- GGG- RRR-). Alternative to r/g/b, and passing both is -32602. A value with bits outside that mask is -32602 — refused, never masked — which this bound alone cannot express." }
        }
      },
      "result": {
        "allOf": [{ "$ref": "#/$defs/replyFields" }],
        "required": ["line", "index", "cramAddr", "value"],
        "properties": {
          "line": { "type": "integer", "minimum": 0, "maximum": 3, "description": "Echoed, so the reply is self-describing without the client re-deriving its own request from an address." },
          "index": { "type": "integer", "minimum": 0, "maximum": 15, "description": "Echoed, with `line`." },
          "cramAddr": { "$ref": "#/$defs/hex", "description": "CRAM byte address written, (line*16 + index)*2 — the spelling emulator/read_cram's entries, pixel_attribution and a cram watch hit all use. D9 category 1." },
          "value": { "$ref": "#/$defs/hex", "description": "The word ACTUALLY STORED — the masked 9-bit colour, so a client reads back what the chip holds rather than what it sent." }
        }
      }
    },
```

**What the fragments carry that the prose cannot.** Five rules are mechanical here rather than advisory:

- The **colour-spelling alternation** is a `oneOf` over the `r`/`g`/`b` triple and `raw`, plus a
  `dependentRequired` binding each component to the other two. Between them they refuse all four bad
  spellings — both at once, neither, a partial triple, and a partial triple beside `raw` — and the last of
  those needs both keywords to fall, which is why the pair is used rather than either alone.
- **`read_cram`'s echo and its array length are tied in both directions** by an `if`/`then`/`else` in
  `result.allOf`: `line` present ⇒ exactly 16 entries, `line` absent ⇒ exactly 64. `read`'s
  region-present-iff-`bus` is the precedent and CR-24's `mode`↔`width`↔`rgb` tie is the standard.
- **Palette entries are `additionalProperties: false`**, so a stray key — an `r8`, say — is a refusal rather
  than a surprise. That is legal there for the reason the reference harness documents: the item subschema
  has no `allOf` to see past.
- **`disp` is bound to `symbol` by `dependentRequired`**, so `{addr, disp}` is refused by the schema and not
  by a sentence.
- **`caveat` is declared ABSENT on both rows**, not omitted by accident — the `sprites` and `write_memory`
  precedent, and §7 decision 1 for why this departs from the recon.

**And one bound the schema cannot express, disclosed rather than glossed.** `raw` is bounded `0 … 3822`
(`0x0EEE`), which is the mechanical half; the exact rule is that **bits outside the mask are `-32602`,
refused and never masked**, and JSON Schema's integer keywords cannot say that. It is carried in the row
prose and in the fragment's `$comment`, standing in exactly the relation `write_memory`'s `value` bound
(0 … 2³²−1, *"must fit width"*) stands to its server-side check — an established precedent on the sibling
row, with a refusal test already in the suite for the analogous case (`write_memory.rs:114`).
`emulator/scanlines`' fragment sets the disclosure precedent, listing its own prose-only constraints under
*"Prose-only, since JSON Schema cannot express them"*.

**The bus-wide params closure.** Every `methods.<name>.params` object gains `unevaluatedProperties: false`:
**26** multi-line objects, **6** one-line `{ "type": "object" }` objects, and the two CRAM fragments authored
closed = **34**, verified by re-parsing (`params NOT closed: []`). `handshake.initialize.params` is
deliberately **not** closed, and that is asserted too. §7 decision 5 explains the keyword choice.

**Executed, not asserted.** The amended document passes `Draft202012Validator.check_schema`; every one of the
68 `params`/`result` fragments compiles standalone with `$defs` spliced; and **48** hand-built messages were
validated against the fragments — **19 accepts, 29 refusals, 0 mismatches**:

| # | Case | Expected |
|---|---|---|
| 1-2 | `write_cram` params: the `r`/`g`/`b` triple; `raw` at the `0x0EEE` ceiling | accept, accept |
| 3-6 | triple **and** `raw`; neither; a lone `r`; a lone `r` beside `raw` | **refuse** ×4 |
| 7-9 | `line` out of range; `index` out of range; `raw` over `0x0EEE` | **refuse** ×3 |
| 10-11 | a component over 7; a missing `line` | **refuse** ×2 |
| 12 | an unknown key (`offset`) — §2.5's closure on this fragment | **refuse** |
| 13 | `raw: 1` — inside the bound, outside the mask: the schema accepts and the **server** refuses (the disclosed gap, tested in the direction it actually behaves) | accept |
| 14-16 | `write_cram` result: happy; missing `value`; the pre-amendment hole, an empty reply | accept, **refuse**, **refuse** |
| 17-18 | §8 item 20's test-time closure over `write_cram`'s result: happy; happy + one surplus key | accept, **refuse** |
| 19-22 | `read_cram` params: omitted `line`; `line: 2`; `line: 4`; an unknown key | accept, accept, **refuse**, **refuse** |
| 23-24 | `read_cram` result: one line echoed with 16 entries; four lines, no echo, 64 entries | accept, accept |
| 25-26 | 16 entries with **no** echo; 64 entries **with** an echo — the tie, both directions | **refuse** ×2 |
| 27-29 | an entry missing `r`; an entry carrying an `r8`; an entry whose `index` is 16 | **refuse** ×3 |
| 30 | a result with no `palette` | **refuse** |
| 31-32 | item 20's closure over `read_cram`'s result: happy; happy + a `truncated` flag §2.4 forbids it | accept, **refuse** |
| 33-35 | `write_memory` params: `symbol`+`disp`; `addr`+`disp`; a negative `disp` | accept, **refuse**, **refuse** |
| 36 | **the probe**: `symbol`+`offset`+`value`+`width` — the payload that succeeded silently | **refuse** |
| 37-39 | the unchanged five-key call; the `bytes` spelling; both payload spellings at once | accept, accept, **refuse** |
| 40-42 | bus-wide closure: an unknown key on `pause`; a valid `read`; `read` with `length` for `len` | **refuse**, accept, **refuse** |
| 43-44 | `watchpoint_add`: a census call (its `if`/`then`-contributed `mode` survives the closure); `censusKey` without census mode still refused | accept, **refuse** |
| 45-47 | `run_to` by symbol; `checkpoint_drop{all}`; `scanlines{startLine,count}` — three untouched fragments still accepting valid calls | accept ×3 |
| 48 | **the carve-out**: `initialize` params carrying two unknown keys | accept |

**And an anti-vacuity control, because a gate nobody has watched reject is a gate nobody has tested.** Ten of
those messages were replayed against the **pre-amendment** schema at `d72513c`. **All ten were accepted
there.** Nine flip to refusal under the amendment — the four bad colour spellings, `raw` over range, the
unknown key, the empty result, the `pause` surplus, the `read` misspelling, and the probe itself — and the
tenth, `{symbol, disp, …}`, stays accepted: before, it was accepted **by silence**; now it is accepted **by
declaration**. That single pair is the whole of 27c's argument in two lines.

No `cargo` was run, at any point — another agent holds the build in that repo — and no emulator MCP tooling
was touched. The only thing executed was JSON.

---

## 7. Pins and decisions

Items 1-7 are places where the recon left two readings, or where this CR reaches a **different** answer than
the recon recommended. Each names the alternative it did not take.

1. **`caveat` is declared ABSENT on both rows.** *(Departs from the recon.)* Recon §4.2 recommends that a
   `write_cram` reply *"carries a `caveat` saying so"* — that the poke bypassed the guest write path — and
   that the amended fragment declare it; recon §1.3 suggests `read_cram` declare one too, *"if the handler
   can emit one (it can — see §5.2)"*, a cross-reference that **does not resolve**: the recon's §5 is the
   withdrawn batch form and has no subsections. Neither is taken, and §2.4 is why. A caveat on **every**
   `write_cram` reply is the constant caveat §2.4's advisory names as the anti-pattern — *"a caveat that is
   always present is documentation wearing signal's clothes"*, with `read_memory`'s constant debug-read
   string as the worked example — and §11.15 applied exactly this reasoning to exactly this class of fact
   **earlier the same day**: a standing property of a VDP-space *hit* *"is stated here, once, rather than
   repeated as a per-reply `caveat`"* (`protocol.md:1030-1033`). So the bypass's two **observable** consequences — the
   watch surface stays silent, and an already-drawn frame is not repainted — are normative prose in the §6
   row, and both fragments say `caveat` is absent on purpose, which is the spelling `write_memory`'s own
   `$comment` already uses (*"caveat is declared absent, not omitted by accident (the sprites
   precedent)"*).
2. **`raw` out of mask is REFUSED, never masked.** *(A choice the recon left to this CR: "decide this in 27a
   and pin whichever way it rules".)* Refused, for §5's stated ethos — a server must not resolve a bad
   request on the caller's behalf — and because the reply's whole job is to report where the write landed:
   a reply carrying a `value` the caller never sent is a silent mutation wearing a success code. The
   alternative not taken is hardware-faithful masking, whose defence is that the reply's `value` would make
   the mutation visible; it loses because *visible* is not *refused*, and because the same reply is the
   client's evidence that its own request was well-formed.
3. **`palette[]` is a flat array of self-describing entries, not the legacy's array of line objects.**
   *(Resolution; the legacy shape is §1.3.)* Three reasons. The legacy reply is pre-bus — a flat `ok: true`
   object in snake_case (`cram_addr`), from a server §8 already records as non-conformant because it stamps
   nothing (`protocol.md:1587-1594`) — so its spellings are not precedent for this bus's camelCase. Its
   nesting makes the two answers two **shapes** where the flat form makes them one shape of two lengths, and
   an entry that carries its own `line` can be handed to `write_cram` on its own. And its `r8`/`g8`/`b8` is
   the unpinned-ramp problem in §1.3. What is kept from it is the part that was right: the `line` filter,
   the per-entry `index`, `raw` and the 3-bit components.
4. **`cramAddr` is carried per entry; `cramIndex` is not.** *(Resolution.)* Both are derivable from
   `(line, index)`, so the `sprites` rule — omit what the reply's own fields derive, which is why that row
   emits no per-entry `satAddr` — argues for omitting both. `cramAddr` earns its place anyway because it is
   the **join key three other surfaces speak and `(line, index)` is not**: `pixel_attribution.cramAddr`, the
   `space`+`addr` pair a `cram` watch hit reports, and `emulator/read` with `space: "cram"` — a pair CR-20's
   fragment names as the reason that row exists at all. `cramIndex` has no such claim: only
   `pixel_attribution` speaks it, and that row emits it because it has no `(line, index)` to give.
5. **The published closure keyword is `unevaluatedProperties`, not `additionalProperties`.** *(Departs from
   the recon, which names `additionalProperties: false`.)* On today's 34 fragments the two are **equivalent**
   — verified by parse: the only property any params applicator contributes is `watchpoint_add`'s `mode`
   `const` inside an `if`/`then`, and `mode` is declared at the parent too — so the choice is about the next
   fragment, not this one. `unevaluatedProperties` is chosen because §8 item 20's own measured lesson is
   that `additionalProperties` **does not see across applicators** (the experiment is reproduced verbatim in
   `protocol.md:1544-1557`), so a fragment that later grows a conditional property would silently begin
   refusing conformant params. Case 43 of the harness is the standing control: `watchpoint_add`'s census
   call still validates.
6. **`disp` is non-negative and `symbol`-only, on `write_memory` alone.** *(Departs from the recon on the
   first; the controller ruled the third.)* The recon proposes it *"signed"* and offers *"the methods that
   already take `addr`/`symbol`"* with `symbol`-only as the conservative half. Non-negative because the
   whole argument for the key is that it **mirrors** `symbolDisp`, which the schema types `minimum: 0` and
   which is by construction a displacement from the *nearest preceding* symbol: `{symbol, symbolDisp}` out,
   `{symbol, disp}` in. A negative displacement addresses a byte the named symbol does not own and nobody
   has asked for it; widening is additive under D5 and belongs to the client that does.
7. **A `lines` count is not emitted.** *(Departs from the recon's §1.3 sketch, "echo `line` (and a `lines`
   count when omitted)".)* The `line` echo's **presence** already carries that fact, the fragment ties it to
   the array's length in both directions, and a `lines` key would duplicate `palette.length` — the
   duplication §2.4 rule 3 and §11.5's `stoppedAtFrame` finding both argue against.

**Carried from the controller's rulings without reopening:** one CR in three parts (recon recommendation A);
`require_paused` on `write_cram` and an ungated `read_cram`; the params policy as `disp` **and** bus-wide
rejection; the EQU-section parser bug and stock-S1 viability out of scope; the batch form recorded as
withdrawn.

**One attribution stated precisely, because the register's failure mode is true-sounding claims.** The
formulation *"the paused machine is where the method earns its keep"* is the **recon's** (§4.1), not a
sentence the demand doc contains. What the demand doc contains is the finding it reads: their two verified
anchors that a direct CRAM write on a **running** Aeon machine *"is overwritten within a frame"* because the
palette is *"composed once per frame into Palette_Buffer"*, and their conclusion that the live-palette demo
is a `write_memory` story. The contract text in §3(c) is written against the finding, generalised off their
engine's name; the recon's phrasing is cited as the recon's.

---

## 8. What does **not** change

1. **The six param names and the two reply keys `write_cram` was catalogued with.** No rename, no redesign.
   The legacy MCP's two tool rows keep their parameter schemas — `read_cram`: `line` only; `write_cram`:
   `line`,`index`,`r`,`g`,`b`,`raw`, required `["line","index"]` (`oracle_mcp.py:801-847`) — and they match
   this row after the amendment exactly as they matched it before, which is a **test obligation** for the
   implementation slice: the MCP must not accept a call the bus refuses.
2. **`read_cram`'s `line` param**, its range and its optionality.
3. **Every result closure in §8 item 20.** §2.5 governs params only; results stay open in the published
   artifact, for D5's reason, and item 20's harness-side closure is untouched.
4. **`emulator/read` at `space: "cram"`, `pixel_attribution`'s `rgb`/`cramIndex`/`cramAddr`, and
   `emulator/scanlines`' rows.** They remain the other ways to ask about a palette entry and none of them
   moves. The one prose consequence — a poke does not repaint a retained `scanlines` frame while
   `pixel_attribution` changes at once — is a statement *about* them in the new row, not an edit *to* them.
5. **§11.10's postscript sentence recording the count mismatch** (`protocol.md:2352-2354`) stays exactly as
   written. An amendment log is a record, not live text — CR-24's precedent, where §11.3's log echo of a
   superseded sentence was explicitly left alone (`protocol.md:2547`).
6. **No currency in the reference repo moves as a consequence of this text.** CR-27 is contract and schema
   only; the arc's currency argument belongs to the implementation slices, where the recon's §9 enumerates
   it as expected-zero by construction — no frozen fixture calls a method that does not exist, and the read
   path is one already exercised by `Engine::read` and `cram_decoded`.

---

## 9. The adoption condition

In CR-24/§11.14's two-part structure — suite gates executable in the reference repo, plus a demand-side
acceptance protocol — and for the reason §11.14 states in the catalog's own voice: *"Registration gated on
an unexecutable clause is a condition that gets waived silently."* (`protocol.md:2581-2582`). The §11.17
entry's closing paragraph is the normative text; this is the same six clauses with their fixtures named.

**Suite gates, executable here:**

1. **Fragment closure, plus one refusal per catalogued bound.** A conformant reply passes **both** fragments
   **open and closed** (`unevaluatedProperties: false` at test time, §8 item 20) — CR-18's condition, whose
   own sentence is the standard here: *"the fragment is registered when a conformant reply **passes it
   closed**, not when it is written"* (`protocol.md:2373-2375`), a rule that in CR-18's own case rejected the
   first real reply it saw. Happy path plus: `write_cram` on a free-running machine → `-32005` with `data.reason = "machineRunning"`;
   `line`/`index` out of range → `-32602`; the triple and `raw` together → `-32602`; a `raw` with bits
   outside `$0EEE` → `-32602` **and the entry unchanged**, which is the refused-not-masked half and the one
   an implementation would otherwise satisfy by masking; `read_cram` with `line` outside 0–3 → `-32602`,
   refused not clipped.
2. **The mechanical ties are each proven by a message that fails them** — a result missing `value`, a partial
   triple, and a `read_cram` reply whose echo and length disagree in **either** direction. §6's harness is
   the schema-side half of this clause and is already executed; the suite half asserts the server produces
   nothing that trips it.
3. **The unknown-param rejection is proven by a probe that passed silently before it.**
   `{symbol, offset, value, width}` → `-32602`, asserted on **both** the typed `error.data.unknownParams`
   and the message text naming `offset`, with a sentinel byte at the target unchanged after the refusal —
   the shape `write_memory.rs:93-129` already uses (sentinel poked first at `:95-107`, the loop at
   `:109-117`, survival asserted at `:122-128`), extended by the one case that loop does not contain. *Mutation requirement:* an assertion that accepts a blanket "some params are
   invalid" satisfies nothing and helps nobody; the offending key must be named.
4. **`disp` round-trips literally.** A `read` reply's `{symbol, symbolDisp}` handed straight back as
   `{symbol, disp}` writes the byte the read named; `{addr, disp}` is `-32602`; a `disp` carrying the target
   past the work-RAM window is `-32004`. The first is the clause that makes D7's round trip a fact rather
   than a claim.
5. **The watch surface stays silent on a `write_cram`** — arm a `cram` watch, poke the entry, assert zero
   hits and `seen` unmoved. This is the direct pin of the row's standing property, and it is the one gate
   that would catch a later "simplification" of the handler into the guest write path.
6. **The two methods read back through each other and through a third instrument** — `write_cram`'s
   `cramAddr`/`value` agree with the same entry as `read_cram` returns it **and** with `emulator/read` at
   `space: "cram"` — against a CRAM state **the test itself established**, since an assertion that still
   passes when the handler returns a fixed palette is vacuous (the `cram_rgb_matches_cram_decoded`
   cross-instrument precedent, `render.rs:1917`).

Clauses 1–6 are executable in the reference repo. Two consequences ride with them and are checkable in the
same run: the advertised count moves **32 → 34**, and the harness's schematized-but-not-advertised set
(`schema_conformance.rs:162-168`) becomes **empty**, which closes §11.10's count-mismatch sentence in fact
while leaving it standing as a record.

**Demand-side acceptance protocol** (not a suite gate — the client is theirs, exactly as §11.14's A1/A2
sweep and §11.16's parity corpus are theirs): **the requester's own offer, taken.** They offered to point
their probe at a branch before it merges, and taking it is the recon's slice 7 — their probe found the
`write_memory` footgun that the reference suite did not. Three properties, and the first is the one under
test:

- **"Advertising a method is shipping it."** Their feature detection runs off `initialize`'s advertised list
  and is live now, so both methods must appear there and light up in their editor **with no coordination and
  no client change**. D4's guarantee is the thing being demonstrated, and the demand doc's own warning is
  the reason it is a gate rather than a nicety: *"it must not do so by loosening what the list means."*
- **The probe that succeeded silently now refuses by name**, against a headless server on the branch — their
  measurement basis, which they flagged themselves as a floor rather than a player-loop figure.
- **A symbol-relative write lands where they meant it to**: `Player_1`+2 through `{symbol, disp}`, verified
  by their own read-back rather than by our reply.

---

## 10. What this CR does **not** carry

Five items, named rather than folded in.

- **The whole-line batch `write_cram`** — **withdrawn by the requester**, same day, with a reason: the drag
  loop it was sized for is not a CRAM loop. Recorded as withdrawn, **not deferred**, and it gets no
  follow-up id, because a deferral is a debt this suite owes and a withdrawal is not (recon §5).
- **`F-PALETTE-DRAG-PACE`** — its registered-but-not-requested neighbour, and the reason the two are
  mentioned together: a `write_memory`-driven palette drag at 30 Hz is 30 pause/write/resume cycles per
  second and **60 `stopped`/`resumed` events per second** to every subscriber. The requester raised it
  themselves and explicitly asked for nothing; their v1 is to throttle to ~10 Hz or coalesce-on-idle and
  measure. The two future shapes they named and did not ask for — relaxing the pause gate for small bounded
  writes, or an apply-at-next-frame-boundary write — are revived by **numbers** or not at all.
- **The `.lst` EQU-section parser defect and stock-S1 listing viability.** Out of scope by ruling: the first
  is a core parser bug in `oracle-next` (the recon measures a healthy sigil listing now reporting 672
  skipped lines, so `is_intact()` is false for a file that is fine — a figure carried from recon §6.3 and
  not re-derived here, since checking it means running the suite), and the second needs a separate ruling about what "bound"
  means for a ROM this project did not build (`F-LST-AS-COLUMNS`, `F-LST-NONDEB2-BINDING`). Neither is
  contract movement and neither belongs in an amendment about the CRAM surface. They are answered in recon
  §6, which is written to be relayed verbatim.
- **`F-CRAM-RAMP`, registered here for the first time.** This CR omits the 8-bit expansion because the ramp
  is unpinned — and the finding is bigger than the row that surfaced it: **`pixel_attribution.rgb` and
  `emulator/scanlines`' `rgb` are already on the wire and already depend on it.** The catalog names the ramp
  exactly once, in `pixel_attribution`'s own description (*"run through the intensity ramp"*,
  `bus-protocol.schema.json:922`), and pins no value anywhere — verified by grep across both contract files.
  Two conformant servers can therefore answer different bytes for the same palette entry on two shipped
  rows. Folding a ramp pin into this CR would smuggle a second amendment under one entry, and the pin needs
  a hardware reference this CR has not measured; **revival condition:** a client comparing two servers' RGB,
  or a hardware-referenced ladder worth adopting.
- **The two papercuts, which are implementation-slice riders and not contract content.** The `$ORACLE_SOCKET`
  refusal surfaces Rust std's *"path must be shorter than SUN_LEN"* unmodified from
  `crates/oracle-aether/src/server.rs:344`, naming neither the limit nor the path — the fix is a pre-bind
  length check in `Server::bind` on the shape of the `AddrInUse` refusal beside it (the recon's figure for
  the usable length is 107 bytes on Linux; not independently verified here). And a connection that asks for
  events and never sends `initialized` sees a healthy socket that never delivers one — one stderr line,
  **once per connection**, at the dispatch arm in the connection loop and not in `Session`, which is pure by
  design. Neither changes a wire shape, so neither is in this amendment.

---

## 11. Claims, anchors, and what could not be verified

**Verified firsthand for this CR** — read in the file named, today, at the revision named:

| Claim | Anchor |
|---|---|
| The two §6 rows read exactly as quoted, in *VRAM / CRAM / layers* | `empyrean/contract/protocol.md:1049-1050`, table `:1044-1057` (pre-amendment, `d72513c`) |
| Both rows, and `write_cram`'s fragment, entered with the commit that introduced the contract, and nothing has touched the fragment since | `git log -S '"emulator/write_cram"' -- contract/schema/…` → one hit, `a25bac1`; `git log --diff-filter=A` on both files → the same commit |
| The pre-amendment fragment reads exactly as quoted | `bus-protocol.schema.json:723-742` @ `d72513c` |
| `read_cram` has no fragment | `methods` parsed at `d72513c`: 34 keys, 33 fragments, no `emulator/read_cram` |
| `methods` holds 33 fragments before / 34 after (34 / 35 keys with `$comment`) | both revisions parsed with `json.load` |
| The reference server advertises 32 methods and none is a CRAM method | `crates/oracle-aether/src/engine.rs:155`ff, `METHODS` parsed → 32 names, zero matching `cram` |
| `UNCOVERED_METHODS` is pinned empty; the schematized-but-not-advertised set is computed and printed with the quoted comment | `crates/oracle-aether/tests/schema_conformance.rs:142`, `:162-168` |
| §11.10's postscript sentence recording the count mismatch | `protocol.md:2352-2354` |
| CR-24's *"amendment logs are records, not live text"* treatment | `protocol.md:2547` |
| §2.4's constant-caveat advisory and rule 4's MUST-declare | `protocol.md:488-495`, `:481-486` |
| §2.4 clause (d)'s structural-bound dichotomy, with `candidates` as the exemplar | `protocol.md:541-547` |
| §8 item 20's text, and the reproduced `additionalProperties`-versus-`unevaluatedProperties` experiment with the *"closure binds servers; additivity protects clients"* reconciliation | `protocol.md:1524-1529`, `:1542-1568` |
| §8's ban on unilateral invention; §9's completed-mechanically note | `protocol.md:1595`, `:1620` |
| §8 records the legacy C++ server as non-conformant (it stamps nothing) | `protocol.md:1587-1594` |
| The run-control state rule's named list and `write_memory`'s reason | `protocol.md:793-803` |
| `write_memory`'s row, its blockquote, and its fragment's mechanical payload alternation | `protocol.md:849`, `:898-910`; `bus-protocol.schema.json:699-712` |
| `read`'s echo rationale, verbatim | `bus-protocol.schema.json:638` |
| `pixel_attribution`'s stored-versus-displayed warning, verbatim, and its `cramIndex`/`cramAddr` pair | `bus-protocol.schema.json:911-912`, `:922` |
| `symbolDisp` is typed `minimum: 0` in the result, and the server emits it beside `symbol` in two places | `bus-protocol.schema.json` (`read`.result), `engine.rs:1271-1274`, `:1427-1429` |
| No `params` fragment sets `additionalProperties: false`; no key-set check exists on the params path | all 33 parsed; `engine.rs:986-1007` (`resolve_target`), `:1286-1300` (`write_memory`'s payload arms) |
| `write_memory`'s refusal loop exercises seven malformed payloads and **no** unknown key | `crates/oracle-aether/tests/write_memory.rs:109-117` |
| The core masks CRAM with `0x0EEE`, lays bytes out with `& 0x7E`, and fires the watch capture on the guest path | `crates/oracle-core/src/vdp.rs:818-826` (mask `:819`, layout `:820`, capture `:823`) |
| `Vdp::cram()` is read-only and already exists | `crates/oracle-core/src/vdp.rs:356-359` |
| The reference server's 3→8 expansion is linear `level × 255 / 7`, is the *only* CRAM decode in the tree, and is tied to the renderer by a test | `vdp.rs:1521-1537`; `render.rs:653-659` (`intensity`, Normal = `level*2 × 255/14`), `:662-678`, `:1917-1926` |
| The legacy server bit-replicates instead, and returns `palette` as an array of line objects in snake_case | `oracle/linux-port/gui/ControlSocket.cpp:2155-2209`, expansion at `:2197-2199` |
| The two expansions differ at exactly three of the eight levels (2, 4, 6) | computed both ways for this CR: 73/72, 146/145, 219/218 |
| The catalog pins no intensity ramp anywhere | grep for `ramp`/`intensity` across both contract files → one description string, `bus-protocol.schema.json:922` |
| The legacy MCP's two CRAM tool rows exist and are hidden only by the advertised-list filter | `oracle/linux-port/mcp/oracle_mcp.py:801-814`, `:815-847`, `:856-876`, `:879-891` |
| The `SUN_LEN` string is Rust std's, propagated unmodified | `crates/oracle-aether/src/server.rs:344` |
| `write_memory`'s poke-is-not-a-guest-access rule and its reason, in the server's own docstring | `crates/oracle-aether/src/engine.rs:1278-1285` |
| §11.15's standing-property-stated-once reasoning, applied to a VDP-space write | `protocol.md:1030-1033` |
| The demand's quoted sentences — the different-halves framing, the withdrawal, *"advertising a method is shipping it"* | `oracle-next/docs/2026-08-19-aurora-client-demand.md:112-113`, `:45-67`, `:190` |
| `protocol.md` is 2724 lines on `main`; §11.15 is the last entry at `:2627`; the amended file is 2973 lines with §11.17 at `:2867` | `wc -l` on both revisions; counted mechanically |

**One anchor drift in the recon, recorded rather than repeated.** Recon §4.2 cites `vdp.rs:817-827` for the
`Target::Cram` arm and quotes it as a block. At the revision read today the arm is `:818-826`; `:817` is the
closing brace of the VRAM arm and `:827` opens the VSRAM one. Every line **inside** the recon's quoted block
is correct, including the `0x0EEE` at `:819` and the `& 0x7E` at `:820`, which it also cites individually and
correctly. Nothing else in the recon's anchors failed re-verification.

**One inherited error corrected rather than repeated.** The demand doc gives `-32011` for the
symbol-not-found refusal its probe received (`aurora-client-demand.md:93`). §5's table spells that code
`-32013` (`protocol.md:733`) and the reference server agrees (`crates/oracle-aether/src/rpc.rs:46`,
`SYMBOL_NOT_FOUND = -32013`); `-32011` is not a code this contract defines at all. The demand's *finding* is
untouched — the composite spelling was correctly refused — and only the number was wrong, but a register
whose failure mode is *"true-sounding claims that later work cites as settled fact"* (§11.10) is one where
an unremarked digit becomes an anchor.

**One dangling cross-reference in the recon, recorded because this CR declines the thing it points at.**
Recon §1.3 defers the `read_cram` caveat question to *"§5.2"*; the recon's §5 is the withdrawn batch form and
has no §5.2. See §7 decision 1.

**Claims this CR does not make.**

- It does not claim the reference server implements any of this. It does not: nothing under `crates/` is
  touched by this arc, and the handlers are the recon's slices 3-5. The contract text is drafted to merge in
  the same window as the implementation.
- It does not claim to have re-run the requester's probe. **No live server was contacted and no emulator MCP
  tooling was used, at any point, for any purpose.** What is verified here is the *mechanism* in source
  (`engine.rs:986-1007`, `:1286-1300`) and the *schema's* permissiveness (parsed), both of which independently
  predict the reported behaviour. The probe results themselves are the demand doc's, relayed.
- It does not claim a test count. The recon's *"none of our 1,588 tests did"* is not re-verified here: **no
  cargo was run in `oracle-next`, by hard constraint** — another agent holds the build, and concurrent runs
  in that tree are known to corrupt. The §8 item 22 text says *"the reference server's whole suite"* rather
  than a number for that reason.
- It does not claim the `SUN_LEN` limit's value. 107 usable bytes on Linux is the recon's figure, carried as
  a rider and flagged as unverified here.
- It does not claim `write_cram` will be implemented by any particular core seam. §3's normative text
  constrains only what is **observable** — the watch surface stays silent, a drawn frame is not repainted —
  and deliberately says nothing about how a server achieves it.

**Nothing is BLOCKED.**

---

## 12. Verification note

**Docs and schema only.** In `oracle-next`, one file under `docs/` — this one — and nothing under `crates/`,
so per the standing rule no `cargo test --workspace` run was required and **none is claimed**; none was
possible either, and the reason is recorded above rather than implied. In `empyrean`, two files
(`contract/protocol.md`, `contract/schema/bus-protocol.schema.json`) on branch `cram-params-amendment`, cut
from `main` at `d72513c`, one commit, **not merged**.

The vendored copy at `crates/oracle-aether/tests/contract/bus-protocol.schema.json` is deliberately
**untouched**: it re-vendors from the empyrean source at the arc's merge window, and editing it now would
fork the vendored artifact from its source and turn
`the_vendored_schema_is_byte_identical_to_the_upstream_contract` red between the two — §11.15's delta and
CR-26 both recorded the same reasoning, and the recon's slice plan puts the re-vendor in slice 3.

**Executed:** `json.load` over both schema revisions, `Draft202012Validator.check_schema` on the amended
document, a standalone compile of all 68 fragments, 48 message validations against the new and amended
fragments, and a 10-message control against the pre-amendment schema. **Not executed:** anything at all in
Rust, and any emulator tooling.
