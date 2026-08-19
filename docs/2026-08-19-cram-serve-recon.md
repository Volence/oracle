# Serving the CRAM surface, and three things found on the way (2026-08-19)

Recon and design for Aurora's demand (`docs/2026-08-19-aurora-client-demand.md`).
**No code was run for this document** — another agent holds the build, so every claim
below is read from source and every number is derived from a file in the tree by
inspection. Where a claim would normally be settled by executing the suite it is marked
**UNVERIFIED-BY-EXECUTION** and the reasoning is shown, so the implementing agent knows
exactly which line to run first.

Trees at transcription: `oracle-next` @ `6e399fd`, `empyrean` @ `d72513c`,
`sigil` @ `0df77f83`, `aurora` @ `677ad66`.

**Implementation is QUEUED** behind the profiler agent's current slice — no concurrent
cargo in this repo. §8 is written so a fresh agent can execute it without this session.

---

## 1. Fragment archaeology — what the two rows actually are today

### 1.1 The count, pinned

Derived by reading `engine::METHODS` and the vendored schema's `methods` object:

| | count |
|---|---|
| methods the reference server advertises | **32** |
| method fragments in the schema | **33** |
| advertised with no fragment | **0** |
| **schematized but not advertised** | **1 — `emulator/write_cram`** |

The vendored copy is **byte-identical** to `empyrean/contract/schema/bus-protocol.schema.json`
(diffed for this document). Aurora's citation of
`crates/oracle-aether/tests/contract/bus-protocol.schema.json:723` is **correct at the
current revision** — that is the first line of the `emulator/write_cram` fragment.

Our own conformance harness already prints this set. `schema_conformance.rs:162-168`
computes it and comments:

> *"Schematized but not advertised: harmless in that direction — D4 makes the advertised
> list authoritative — but worth printing, because it is the shape of a method we might be
> missing."*

It is currently a one-element list, and that element is the last living survivor of the
count mismatch §11.10 recorded (`protocol.md:2352-2354`):

> *"a schema count wrong on both ends (the schema holds 26 fragments and the reference
> server advertises 25; `write_cram` is schematized and not served)"*

That sentence is the divergence note, and it lives in the CR-18 adjudication postscript —
in a passage whose whole subject is *evidence that was wrong*. Closing it touches: the
§6 row (`protocol.md:1049-1050`), the two schema fragments, the vendored copy, the
harness's `schema_only` list, and nothing else. The `protocol.md:2353` sentence itself is
an **amendment-log record and stays untouched** — CR-24's precedent, where §11.3's log echo
of a superseded sentence was explicitly left alone because *"amendment logs are records,
not live text"* (`protocol.md:2547`).

### 1.2 `emulator/write_cram` — a pre-CR fragment, and it shows

`git log -S` on the canonical schema puts this fragment in the **original Phase-1 contract
commit** (`empyrean` `c9aaf16`/`a25bac1`). It has been touched by no CR since. It therefore
predates:

- **§11.5's result-key pass** (2026-08-15) — which is why it has no `$comment` naming its
  §6 provenance, while every fragment added or revised since does.
- **§2.4's conventions** (2026-08-15) — rule 4: *"A schema fragment, however, MUST declare
  `caveat` for any method that can emit one, because §8 item 20 closes results against
  their fragments at test time and an undeclared caveat would fail that check"*
  (`protocol.md:481-486`).
- **§8 item 20's closure** (2026-08-15) — the harness validates every reply with
  `unevaluatedProperties: false` against the fragment.
- **D9's four categories** as they now read, including category 4 (added §11.2).

The fragment, verbatim from `bus-protocol.schema.json:723-742`:

```json
"emulator/write_cram": {
  "params": {
    "type": "object",
    "required": ["line", "index"],
    "properties": {
      "line":  {"type":"integer","minimum":0,"maximum":3},
      "index": {"type":"integer","minimum":0,"maximum":15},
      "r": {"type":"integer","minimum":0,"maximum":7},
      "g": {"type":"integer","minimum":0,"maximum":7},
      "b": {"type":"integer","minimum":0,"maximum":7},
      "raw": {"type":"integer"}
    }
  },
  "result": {
    "allOf": [{"$ref":"#/$defs/replyFields"}],
    "properties": {
      "cramAddr": {"$ref":"#/$defs/hex"},
      "value":    {"$ref":"#/$defs/hex"}
    }
  }
}
```

Four concrete non-conformances with how CR-era methods spell things:

1. **The result declares no `required`.** Every CR-era result fragment does —
   `read`'s (`:637`), `write_memory`'s (`:716`, `required: ["addr","len"]`),
   `pixel_attribution`'s (`:896`), `sprites`', `scanlines`'. Under item 20's closure,
   `unevaluatedProperties: false` catches a **surplus** key and says nothing about a
   **missing** one — so as written, a `write_cram` that returned `{}` would pass the gate.
   A row whose entire job is to confirm where the write landed must require both keys.
2. **`raw` is an unbounded integer.** CRAM is a 9-bit colour; the core masks with `0x0EEE`
   (`crates/oracle-core/src/vdp.rs:819`). The fragment permits `raw: 70000`, which the
   server must then either refuse (a rule the schema does not carry) or silently mask (the
   silent-mutation shape item 3 of the demand is about).
3. **`r`/`g`/`b` vs `raw` exclusivity is unenforced.** The §6 row spells it as an
   alternation — ``` `r`/`g`/`b`\|`raw` ``` — and the fragment expresses none of it. Both
   at once is permitted; so is `r` alone with no `g`/`b`. `write_memory`'s fragment is the
   house precedent for exactly this shape and spells it mechanically
   (`:699-702`: an `allOf` of a `oneOf` over `bytes`/`value` plus a `dependentRequired`
   tying `value`↔`width`), with the server refusing the both-at-once case in words
   (`engine.rs:1291-1294`). CR-24 set the standard explicitly: the `mode`↔`width`↔`rgb`
   tie is *"enforced mechanically by an `if`/`then` in `result.allOf` rather than left to
   prose"* (`protocol.md:2547`).
4. **No `$comment`, no D9 annotation, no echo of `line`/`index`.** CR-20's `read` states
   the house reason for echoing: *"Echoed, so a reply is self-describing: `addr` means
   nothing without the space it is in"* (`:638`). `cramAddr` alone makes the client
   recompute `line`/`index` from an address to confirm its own request.

**Verdict: the shape is sound and the fragment is not. Amend before serving.** Note that
none of this is a redesign — `line`/`index`/`r`/`g`/`b`/`raw` → `cramAddr`/`value` is the
right surface, and it is the surface the old C++ Oracle already serves and the MCP already
has a tool row for.

### 1.3 `emulator/read_cram` — there is no fragment

Aurora described both methods as schematized. **`read_cram` has a §6 row
(`protocol.md:1049`) and no schema fragment at all** — it is not among the 33. Serving it
takes the fragment count 33 → 34, which is the same movement CR-24 made for `scanlines`
(*"Schema gains `emulator/scanlines` — 32 fragments → 33"*, `protocol.md:2547`).

The row is one column wide: `line`? (0–3) → `palette[]`. Two things it does not say:

- **What `palette[]` contains when `line` is omitted.** 64 entries flat? Four arrays of
  16? The old C++ server's MCP tool row says *"Omit to read all 4 lines"*
  (`oracle/linux-port/mcp/oracle_mcp.py:801-814`) without saying what shape comes back.
- **What one element is.** A hex CRAM word (D9 category 1)? An `{r,g,b}` object of stored
  0–7 components? A displayed 0–255 RGB triple? These are three different answers and
  `pixel_attribution`'s fragment already warns about confusing the last two:
  *"NOT the stored CRAM components — `emulator/write_cram`'s r/g/b (0-7) are the stored
  colour, this is the displayed one, and the two differ whenever `state` is not 'normal'"*
  (`bus-protocol.schema.json:922`).

**Does `palette[]` conform to §2.4's container rules?** Yes, and specifically it takes
**neither** the bounded-list envelope nor a cursor. §2.4(d) draws the line at
policy-vs-structural: *"policy bound → flag it, and cursor it only where continuation is
supported; structural bound → neither"*, with `pixel_attribution`'s `candidates` named as
the structural exemplar — *"bounded at 4 by the video hardware, with no partial-list
failure mode to protect against"* (`protocol.md:549-552`). CRAM is 4×16 by the hardware.
A `palette[]` of 16 (or 64) is structurally bounded, cannot be truncated, and so must
**not** carry `total`/`returned`/`truncated`. It is also **not** a "container object"
under §2.4's two-spellings rule: it is a plain array field beside its siblings, exactly
like `candidates`.

So `read_cram`'s shape needs *specification*, not correction — there is nothing yet to be
wrong. The recommendation is a fragment mirroring the CR-era conventions: echo `line`
(and a `lines` count when omitted), one documented element type, and a declared `caveat`
if the handler can emit one (it can — see §5.2).

### 1.4 Verdict

**Both rows need contract movement before either can be served conformantly.**
`write_cram`'s fragment needs amendment; `read_cram` needs a fragment written. The bus
surface itself — the param names, the reply keys — is right and is not being redesigned.

---

## 2. Contract vehicle — does serving need a CR?

**Serving alone: no.** §8's prohibition runs the other way — *"What the Oracle side must
not do: invent new ops not in this spec"* (`protocol.md:1595`) — and §9's standing note
says the remaining fragments are *"completed mechanically during conformance (§8 step 2)
from §6"* (`protocol.md:1620`). Implementing a catalogued, schematized row is the
compliant direction, not a deviation.

**Precedent hunt, and it comes up empty in an informative way.** Every fragment that has
moved has moved *schema-follows-server*: §11.5 added twelve fragments *"chosen to complete
the reference server's advertised surface rather than the catalog's"* (`protocol.md:1618`);
§11.6 found five registrations that never reached the schema; CR-10, CR-18 and CR-24 each
created a **new row** for a capability the core already had. **No method has ever gone
catalogued-and-schematized → served**, because `write_cram` is the only one that has ever
been in that state. There is no precedent to follow, and its absence is not an obstacle.

**But amendments are needed anyway (§1.4), so a CR is the vehicle.** Recommend **one small
CR-27** carrying three items:

| item | movement |
|---|---|
| **27a** `write_cram` fragment amendment | `required: ["cramAddr","value"]`; bound `raw` to `0x0EEE`; express the `r`/`g`/`b`↔`raw` alternation mechanically (`write_memory`'s `oneOf`+`dependentRequired` precedent); echo `line`/`index`; add the `$comment`. Count unchanged at 33. |
| **27b** `read_cram` fragment + row disambiguation | New fragment (33 → 34); the §6 row gains the omitted-`line` shape and the element type. |
| **27c** the params policy | A normative statement on **unknown top-level params** (§4). This is the one genuinely new rule; the contract says nothing about the client→server direction today. |

**Options the controller rules between:**

- **(A) One CR-27 with all three.** Recommended. They are one sitting's work, they all
  touch the same client's report, and 27c is the item most likely to attract a ruling —
  bundling it with two mechanical fragment fixes gets it read.
- **(B) Split: a mechanical CR-27 (27a+27b) and a separate CR-28 for the params policy.**
  Defensible if the controller wants the CRAM rows unblocked while 27c is argued. Costs a
  second adjudication.
- **(C) Serve first, amend after.** **Not recommended.** It would put a reply on the wire
  that item 20's closure binds against a fragment that does not describe it — which is
  precisely the drift item 20 exists to catch, and which §11.10 measured at *"result keys
  on the wire across 16 of the methods it advertises"* (`protocol.md:1571`).

---

## 3. The `write_memory` params policy (demand item 3)

### 3.1 What the server does today, generally

There is **no unknown-param check anywhere on this bus.** Every handler reads the keys it
knows by name and ignores the rest. `resolve_target` (`engine.rs:986-1007`) reads `symbol`,
else `addr`; `write_memory` (`engine.rs:1284-1355`) reads only `bytes`/`value`/`width`
beyond that. Aurora's probe is right about the behaviour and right about the cause.

**What the schema says:** no `params` fragment sets `additionalProperties: false` —
checked across all 33. The published schema therefore *permits* surplus params on every
method. A server that rejected them would be **stricter than the artifact D14 makes the
wire authority**, which is the reason 27c is a contract item and not just a code change.

**What JSON-RPC 2.0 says:** nothing. It defines `-32602 Invalid params` and leaves the
determination of validity to the method.

**What the house already decided about the mirror-image question** is the strongest guide
available, and it comes from §8 item 20. Closure of *results* was deliberately **not** put
in the published schema, and the reasoning transfers with the subject reversed
(`protocol.md:1560-1568`):

> *"Closure in the published artifact would weaponise **stale schemas against conformant
> servers** — D5's preserved-defect argument inverted… The reconciliation is that the two
> obligations have different subjects. **Closure binds servers; additivity protects
> clients.** So the closure belongs in the one place where only servers stand: the
> conformance harness."*

For params the subjects swap: the sender is the client and the validator is the server.
D5's additivity protects a *client* reading a server's replies; it does not entitle a
client to send keys the server never registered. And there is no stale-artifact hazard in
this direction, because the server validating is the same server that publishes its own
advertised surface. **The asymmetry is real and it argues for closure on params.**

### 3.2 Aurora's sharpening, and why it changes the option set

Quoted from the relay:

> a `disp` param and reject-unknowns answer **different halves** — the footgun half is
> what bit them.

Taken seriously, this rules out the two single-measure options as *complete* answers. A
`disp` param alone leaves the next client's `offset:` guess exactly as silently wrong.
Rejection alone leaves them computing hex addresses in the editor, against D7's whole
point — and D7 is a property they explicitly asked us to keep.

### 3.3 The options

**(a) Reject unknown top-level params on `write_memory` only.**
*Blast radius: nil.* The MCP's `write_memory` tool declares exactly
`addr`/`symbol`/`bytes`/`value`/`width` and no more
(`oracle/linux-port/mcp/oracle_mcp.py:307-323`), forwards `params` verbatim with no
injection (`:931`, `:967`), and the MCP SDK validates against a generated schema carrying
`additionalProperties: false` (`:895`) *before* the call leaves the client. Every
`write_memory` call site in this repo — `tests/write_memory.rs` (13 sites),
`tests/memory_hash.rs:52`,`:163` — draws only from those five keys; `oracle-replay`,
`oracle-frontend` and `tools/` make none. `oracle/linux-port/mcp/coverage_check.py:50`
sends `{addr, bytes}`. **Nothing breaks.** *Cost:* one method behaves unlike the other 31,
and the next footgun lands on `read_memory` or `run_to`.

**(b) Reject on ALL methods.**
*Blast radius: still nil against every client we can see* (the MCP's per-tool schemas are
all closed, and the forwarder adds nothing to any method). *Cost:* it is a behavioural
change to 32 methods at once, it is a **breaking change for any client we cannot see**,
and it is the change most likely to need a staged rollout (warn-then-reject). It is also
the honest generalisation: item 3 is not a `write_memory` bug, it is a bus-wide silence.

**(c) Add an explicit `disp` param (additive) AND reject unknowns.** **RECOMMENDED.**
Two halves for two halves. `disp` is additive and breaks nothing; it mirrors the output
convention already on the wire — `read`/`read_memory` reply with `symbol` +
`symbolDisp` (`engine.rs:1271-1274`, `:1426-1429`), so a client that receives
`{symbol:"Player_1", symbolDisp:2}` can hand the identical pair back. That symmetry is
worth more than the keystrokes: it makes the round trip literal, which is what D7 asks a
client to rely on. Rejection then makes the guessed spellings loud.

**Recommended shape of (c):**
- New optional `disp` on the methods that already take `addr`/`symbol` — integer, D9
  category 2 (a displacement is a thing you count with), signed, applied after resolution.
  Whether it is valid with `addr` as well as `symbol` is a ruling; the conservative answer
  is `symbol` only, because `addr` + `disp` is just arithmetic the client already did.
- Unknown-param rejection scoped per **(b)**, staged: land the check behind the same
  helper on every handler, but **register the change as a contract item (27c)** so the
  published `params` fragments gain `additionalProperties: false` in the same movement —
  otherwise D14 makes the schema right and the server wrong.
- `-32602`, naming the offending key **and** listing the accepted ones, so the message
  itself is the fix. The house already writes refusals this way
  (`engine.rs:1291-1294`, `:1310-1315`).

**Note the coverage gap the implementing agent inherits:** `tests/write_memory.rs`'s
`payload_spelling_is_exactly_one_of_two` loop (`:109-117`) exercises seven malformed
payloads and **not one probes an unknown key**. There is no existing test that would go
red today and none that pins the new behaviour. Tests-first is mandatory here.

---

## 4. Handler design

### 4.1 The pause question — answered by the requester, not just by precedent

`write_cram` **is `require_paused`.** Aurora settled it from their own side: the unpaused
case fails for **engine** reasons, not server reasons (a composed-per-frame palette
pipeline overwrites a direct CRAM write within a frame — `aeon/engine/effects/palette.emp:31`,
`aeon/engine/ram.emp:973`), and where `write_cram` earns its keep is the **paused** machine
— inspect a colour, tweak it, see it on glass without the pipeline stepping on it. That is
exactly the shape `require_paused` gives. **This is demand-side confirmation, not
precedent-following**, and it is worth recording as such: the gate is what the one client
who asked for the method wants, which is a much better argument than symmetry with
`write_memory`.

Precedent agrees anyway. `write_memory`'s docstring (`engine.rs:1279-1283`):

> *"requires a paused machine (named in §6's run-control state rule for `press`'s reason —
> a poke mid-free-run mutates the timeline just as surely)"*

`read_cram` is **not** gated — a pure read, per the `scanlines`/`read`/`pixel_attribution`
precedent. `read` states it plainly: *"A pure read: no `require_paused`"*
(`engine.rs:1362`), and `pixel_attribution`'s docstring gives the full reason
(`engine.rs:1461-1466`): §6's run-control rule names ops that mutate the timeline, a read
mutates nothing, and D11's stamp (`running: true` on every reply) is the torn-instant
warning. Aurora confirmed ungated is what they want.

### 4.2 The core seam — and a choice with a house rule pointing both ways

`Vdp::cram()` is **read-only** (`crates/oracle-core/src/vdp.rs:356-359`); the `cram` field
is private and is written in exactly one place, `Vdp::write_target`'s `Target::Cram` arm
(`vdp.rs:817-827`):

```rust
Target::Cram => {
    let masked = w & 0x0EEE;           // 9-bit colour (---- BBB- GGG- RRR-)
    let b = (self.addr as usize) & 0x7E;
    let old = ((self.cram[b] as u32) << 8) | self.cram[b | 1] as u32;
    self.capture(VdpTarget::Cram, b as u32, old, masked as u32, 2);
    self.cram[b] = (masked >> 8) as u8;
    self.cram[b | 1] = (masked & 0xFF) as u8;
}
```

`read_cram` needs nothing new — `cram()` is already there and `Engine::read` already uses
it for `space:"cram"` (`engine.rs:1399`).

`write_cram` needs a seam, and there are two, with the house arguing on both sides:

- **Drive the real VDP port path.** `write_memory`'s stated principle:
  *"Bytes travel the bus path, so hardware mirror masking applies and no `ram_mut` debug
  back door exists on core"* (`engine.rs:1281-1282`). And `debug_read`'s docstring names
  the sibling's opposite choice as the landmine to avoid: *"exactly the landmine the recon
  found in the sibling's `write_vram`, which bypasses the VDP port path and 'nothing in
  its docstring says so'"* (`engine.rs:1014-1016`).
- **But the port path fires the watch surface**, via `self.capture(VdpTarget::Cram, …)`.
  That directly contradicts `write_memory`'s other stated principle
  (`engine.rs:1283-1286`): *"The sink is `()` on purpose: a poke is a debugger access, not
  a guest access — it is never offered to the watch surface, because a hit's `pc` names
  the instruction that drove the access and a poke has none to name."* A debug `write_cram`
  routed through `write_target` would put a hit in a client's watch ring with no
  instruction behind it. **CR-25 sharpens this into a second problem:** since §11.15 a
  captured CRAM write carries its instruction's start clock and gets a **landing pixel**
  from it (`protocol.md:2640`, `:2671`). A poke has no instruction and therefore no
  landing clock; feeding one into the sub-line model would either fabricate a landing or
  silently take whatever `mclk` happens to be current.

**RECOMMENDATION: a narrow, explicitly-named core seam that bypasses the port path but
replicates its arithmetic exactly** — the `0x0EEE` mask and the `& 0x7E` big-endian byte
layout, lifted from `write_target`'s Cram arm so the two cannot drift — and that does
**not** capture. Something like:

```rust
/// Debug poke of one CRAM entry. Bypasses the port path (no FIFO, no autoincrement, no
/// DMA) and deliberately does NOT capture to the watch surface: a debugger write has no
/// instruction to name, and since §11.15 a captured CRAM write needs a landing clock a
/// poke cannot supply. Masks and lays out bytes exactly as `write_target` does.
pub fn poke_cram(&mut self, index: u8, word: u16) -> u16 { … }
```

Returning the **stored** (masked) word is what lets the reply's `value` be truthful
without the engine re-deriving the mask. And because the bypass is real, the reply
**carries a `caveat` saying so** — which the amended fragment must then declare (§2.4
rule 4), and which the `write_vram` landmine says out loud what the sibling never did.

`require_paused` blunts the fidelity objection considerably: with the machine stopped there
is no FIFO in flight, no DMA in progress and no active raster, so the port path's
side effects are exactly the ones a paused poke has no business producing.

**One consequence to document, not to fix:** `emulator/scanlines` retains the last
completed frame (`engine.rs:1741`, `source: "raster"`). A `write_cram` on a paused machine
changes CRAM but does **not** repaint a retained frame, so `scanlines` will keep reporting
the pre-write colours until the machine advances, while `pixel_attribution` (which
re-derives from live state) will change immediately. That is the same
retained-vs-re-derived split CR-24 and CR-10 already document; it needs a sentence in the
row, not a code change.

### 4.3 Four-surface accounting (D15)

| surface | disposition |
|---|---|
| **Bus row** | Exists for both (`protocol.md:1049-1050`). Amend per §2, then serve. |
| **Schema** | `write_cram` amend; `read_cram` create. 33 → 34. |
| **MCP tool** | **Nothing to do — it lights up by itself.** The MCP filters its tool list against the server's advertised `methods` from `initialize`: `served_methods()` (`oracle_mcp.py:856-876`) returns `set(bus.methods)`, and `list_tools()` skips any tool whose `emulator/{op}` is absent (`:879-904`, the skip at `:890`). The `emulator_read_cram` (`:801-814`) and `emulator_write_cram` (`:815-847`) rows **already exist** and are hidden today purely because oracle-next does not advertise them. The moment we advertise, they appear. No allow-list, no version gate. *This is also a test obligation:* the tool schemas the MCP already publishes (`read_cram`: `line` only; `write_cram`: `line`,`index`,`r`,`g`,`b`,`raw`, required `["line","index"]`) must match what we serve, or the MCP will accept a call the bus refuses. They do match the §6 row today — keep it that way through 27a. |
| **Player GUI** | **None, by decision.** The frontend already renders the whole CRAM as a swatch grid built from `Vdp::cram_decoded()` in-process (`crates/oracle-frontend/src/lens/video.rs:54-69`, with the grid/array agreement asserted at `:40-46`). D15's parity rule runs *bus-before-panel* — *"a server SHOULD expose through the bus every capability its own GUI consumes"* (`protocol.md:240-242`) — and serving `read_cram` **satisfies** that direction: the panel's capability finally gets its bus row. Adding an *editing* affordance to the player is a separate product decision with no demand behind it; the requester is an editor. Recorded as a decision, not an omission, per the standing three-surface rule. |

---

## 5. Batch form (demand item 4) — withdrawn, not deferred

Aurora **withdrew** this the same day, with a reason: the drag loop it was sized for is
not a CRAM loop (see the demand doc §1, item 4). It is therefore **not** registered as
debt and gets no `F-` number — a deferral is something we owe and a withdrawal is not.

For the record, had it stood, the recommendation would have been defer-and-register:
16 singles at 30 Hz over a unix socket is 480 round trips/second on a transport doing
newline-delimited JSON to a local peer, and nothing in the fragment shape makes a batch
form trivially additive (a `entries[]` alternative would need its own `oneOf` against
`line`/`index` and its own bounded-list decision). The measurement that would have revived
it: a client showing sustained per-call latency such that 16 calls miss a 33 ms budget.

---

## 6. Q6 — what `load_symbols` actually checks, and the stock-Sonic-1 answer

**★ This section is written to be relayed to the Aurora session verbatim. It is
self-contained.**

### 6.1 The check, exactly

`load_symbols` (`crates/oracle-aether/src/engine.rs:2111-2177`) does three things in order:

1. **Read the file.** Unreadable → `-32602`, `data: {path}`.
2. **Parse it** with `SymbolTable::parse` (`crates/oracle-core/src/symbols.rs:328-384`).
   The only fatal parse error is `NoSymbols` — *"no symbols found (not a sigil/AS `.lst`
   listing?)"* (`symbols.rs:228`). Individual unrecognised lines are **never** fatal; they
   are skipped and counted in `skipped_lines`.
3. **Bind it to the loaded ROM** with `SymbolTable::validate_against_rom`
   (`symbols.rs:622-651`).

**The binding check keys on exactly one thing: a symbol literally named `EndOfRom`,
and two bytes of the ROM at that offset.** Verbatim from its docstring
(`symbols.rs:614-618`):

> *"every sigil-built Aeon ROM carries a `deb2` symbol appendix appended at `EndOfRom`,
> and `EndOfRom` is a symbol *in the listing*. So the listing names an offset, and the ROM
> either has the magic there or it does not. No new format needs decoding — we read two
> bytes."*

The magic is `DE B2` (`symbols.rs:275-277`) — *"byte-for-byte the check the on-target MD
Debugger blob itself performs (`cmpi.w #$DEB2,(a1)+`) to find its own symbol table"* — and
the appendix must be at least `0x2000` bytes (`symbols.rs:282`).

The three outcomes and what `load_symbols` does with each:

| `EndOfRom` | bytes at that offset | verdict | `load_symbols` |
|---|---|---|---|
| present | `DE B2`, ≥ 0x2000 to EOF | `Match` | **accept**, with a caveat |
| present | offset past EOF | `Mismatch(EndOfRomOutOfRange)` | **refuse** `-32602` |
| present | not `DE B2` | `Mismatch(NoAppendixMagic)` | **refuse** `-32602` |
| present | `DE B2` but < 0x2000 to EOF | `Mismatch(AppendixTooSmall)` | **refuse** `-32602` |
| **absent** | — | `Indeterminate(NoEndOfRomSymbol)` | **depends on `is_intact()`** |

The last row is the one that decides your case. `load_symbols` (`engine.rs:2144-2160`)
splits it:

- `Indeterminate` **and** `is_intact()` → **accepted unverified**, with
  `binding: "indeterminate"` and the caveat *"this listing declares no EndOfRom, so it
  could not be checked against the loaded ROM at all. Accepted unverified because it is
  internally intact."*
- `Indeterminate` **and not** `is_intact()` → **REFUSED**, `-32602`,
  `data: {binding: "indeterminate-and-damaged"}`, message *"cannot be bound to the loaded
  ROM and is not internally intact, so it cannot be trusted"*. The in-code comment calls
  this *"Fail-open closed (recon §9g): a listing that WOULD be refused becomes merely
  Indeterminate once its EndOfRom row goes missing, and truncation removes rows from the
  end — where EndOfRom sits."*

`is_intact()` (`symbols.rs:497-501`) is four conditions, **all** required:
the table came from the `Symbol Table` section (not the body-line fallback), a `N symbols`
footer exists, the footer's count equals the number of rows parsed, and **`skipped_lines`
is zero**.

**The 92.6% figure you asked about** is at `engine.rs:2124-2127`, and it is why the check
exists at all:

> *"the listing is validated against the image actually loaded, and a listing from a
> different build shape is REFUSED. Of the symbols `s4.lst` and `s4.debug.lst` share,
> **92.6% name a different address** — a mismatched listing is not degraded information,
> it is confidently wrong information."*

And the honest limit, from the same docstring set (`symbols.rs:625-633`): `Match` means
*"not obviously wrong"*, never *"proven right"* — `demo.lst` and `demo.debug.lst` both
declare `EndOfRom : 11224`, so the probe cannot separate them.

### 6.2 The stock Sonic 1 answer: **it will be refused, and the reason is not the check you asked about**

**There are two walls, and you hit the first one before you reach the binding check.**

**Wall 1 — AS packs two symbols per line, and our row parser accepts exactly one.**

`parse_table_row` (`symbols.rs:389-408`) tokenises a `Symbol Table` row on whitespace and
requires **exactly five tokens**: `NAME : HEXADDR <C|-> |`. AS's own `-L` output puts
**two entries on every line**. Ground truth from a real AS listing in this workspace
(`skdisasm/sonic3k.lst:375449`):

```
 EndOfROM :                  3345A0 C |  EndSign_CheckPlayerHit :     83960 C |
```

Ten tokens. Skipped. Simulating our exact parser (including `u32::from_str_radix`, which
also rejects AS's sign-extended 16-hex-digit RAM values) over two real AS listings:

| listing | declared | rows we parse | skipped | `EndOfRom` parsed? |
|---|---|---|---|---|
| `skdisasm/sonic3k.lst` (AS/asw, S3K) | 39,074 | **1,060** (2.7%) | 19,372 | **no** |
| `sonic_hack/S4.lst` (AS/asw, S2-based) | 6,739 | **465** (6.9%) | 3,436 | **no** |

The rows that *do* parse are the accidental ones — a name long enough to push its
line-mate off, leaving a single entry. So the parse **succeeds** (it is not `NoSymbols`),
and hands back a **silent, arbitrary 3–7% subset** of the symbol table.

`EndOfRom` almost certainly is not in that subset. So: `Indeterminate(NoEndOfRomSymbol)`,
and `is_intact()` is `false` (thousands of `skipped_lines`, count mismatch) →
**`-32602`, `binding: "indeterminate-and-damaged"`.**

**Expect exactly this message:**
`"<path> cannot be bound to the loaded ROM and is not internally intact, so it cannot be
trusted"`.

Note the shape of the escape: the fail-open-closed guard is what saves you, and it fires
for the *right* outcome via the *wrong* reason — it thinks the file is damaged when the
file is fine and our reader is not.

**Wall 2 — even with wall 1 fixed, stock S1 does not carry a `deb2` appendix.**

Stock Sonic 1 does define the symbol, and spells it with our exact casing —
`s1disasm/sonic.asm:5235` is `EndOfRom:`, with `sonic.asm:184`
`RomEndLoc: dc.l EndOfRom-1` putting the header's last-address longword one byte below it.
(skdisasm spells it `EndOfROM`, which our exact-match lookup would *not* find; S1 is the
lucky spelling.) So `EndOfRom == rom.len()` for an unpadded build, and:

- **unpadded** → `rom.get(off..off+2)` is `None` → `Mismatch(EndOfRomOutOfRange)` → refused.
- **padded** (`padToPowerOfTwo`) → the bytes at `EndOfRom` are pad (`$00`/`$FF`), not
  `DE B2` → `Mismatch(NoAppendixMagic)` → refused.

Either way: **refused, positively**, not merely unverified.

**So: stock-S1 AS listings are not viable today, and making them viable needs two changes,
not one.** (i) a `Symbol Table` row reader that splits a line on `|` before tokenising —
prototyped for this document: it lifts `sonic3k.lst` from 1,060 to **38,388** of 39,074
declared symbols and does find `EndOfROM = 0x3345A0`; and (ii) a **policy** for a ROM with
no `deb2` appendix, which is a ruling, not a patch. The honest options are an
`Indeterminate` variant meaning *"this listing has a fingerprint symbol but this ROM has no
appendix to check it against"* (accept-unverified, loudly), or a different binding key for
non-Aeon images. Both are registered below as `F-LST-AS-COLUMNS` and
`F-LST-NONDEB2-BINDING`. Neither is scheduled; both are cheap and both are gated on a
ruling about what "bound" means for a ROM this project did not build.

### 6.3 The third section — sigil's new `Equate Table`, verified at source

Your relay (via the Aeon session) is **correct and it is four hours old**. Verified in
`sigil`: `0c56ba10` *"feat(listing): equates reach the .lst — an Equate Table section"*,
2026-08-19 16:21, merged as `0df77f83` 16:22, **both ancestors of sigil HEAD**. The
listings in the Aeon tree were rebuilt right after (`aeon/s4.lst` mtime 16:24,
`aeon/s4.debug.lst` 16:23) and now carry it:

```
  Equate Table (name = value; values, not addresses):

EQU AF_BACK = $000000FE
…
   670 equates
```

670 `EQU` rows in `s4.lst`, **671** in `s4.debug.lst` — matching your relayed figure
exactly. The pre-change snapshot at `.aeon-nightly/s4.debug.lst` (04:40 today) has zero,
which dates the change precisely.

**The three failure modes you ranked by quietness — which one actually happens:**

**(a) Does our parser ingest EQU rows as address symbols? NO.** This is the good news and
it is the one you ranked quietest. Once the `Symbol Table` header is seen, `parse` sets
`in_table` and every later line goes to `parse_table_row`, which requires exactly five
whitespace tokens with `tok[1] == ":"` and `tok[4] == "|"` (`symbols.rs:401`). An
`EQU AF_BACK = $000000FE` row tokenises to **four** tokens with `tok[1] == "AF_BACK"`. It
is rejected and counted as skipped. **`lookup_symbol` cannot return an equate; addr→name
cannot return one as "nearest preceding label".** Confirmed the other way too: the symbol
table in the new `aeon/s4.lst` holds 2,216 rows and **every one is type `C`** — the
equates are not double-listed into it.

Say plainly why we got lucky: **no code in this repo knows the Equate Table exists.** The
rejection is a side effect of a strict five-token row shape, not a decision. That is worth
fixing deliberately (below), because the next emitter change may not be so kind.

**(b) Does `validate_against_rom` key on anything 671 extra rows move? NO** — directly. It
reads `address_of("EndOfRom")` and two ROM bytes and nothing else; row counts do not enter
it. **But `is_intact()` DOES flip to false**, because `skipped_lines` goes from 0 to
**672** (`s4.lst`: the section header + 670 `EQU` rows + the `670 equates` trailer; the
`0 unused symbols` line is consumed by the footer parser and is not skipped). `is_intact()`
is consulted only on the `Indeterminate` branch, which a sigil listing never takes — it has
`EndOfRom` and the appendix is there, so it lands on `Match`. **So no live refusal today.**
The latent edge is real and narrow: any listing that both lacks `EndOfRom` **and** carries
an Equate Table now gets refused as *"damaged"* when it is not.

**(c) Does a strict parser reject the third section outright? NO.** `parse` returns `Err`
only for `NoSymbols`.

**What the auto-load paths do on first contact with a new-format file: nothing different.**
All three consumers — the bus (`engine.rs:2128-2161`), the player
(`crates/oracle-frontend/src/symbol_file.rs:60-95`) and the replay policy
(`crates/oracle-replay/src/policy.rs:52-78`) — apply the same table and all three land on
`Match` → **accept**. **Live use is not broken.**

**(3) Fixture exposure — and it is the *inverse* of the silent-divergence shape you were
worried about.** There are **no vendored `.lst` fixtures in oracle-next at all**.
`crates/oracle-core/tests/symbols_real_lst.rs` reads the **live Aeon tree** directly:
`aeon_dir()` (`:23-27`) is `$ORACLE_AEON_DIR` or the hardcoded
`/home/volence/sonic_hacks/aeon`. So the suite does **not** stay green while live use
breaks — it goes **red while live use keeps working**, which is the better failure and the
one we have:

> **UNVERIFIED-BY-EXECUTION (no cargo available to this session).** Reasoned from source:
> `real_s4_lst_parses_completely` asserts `t.skipped_lines() == 0` (`:66-70`) and
> `t.is_intact()` (`:77`) against `aeon/s4.lst`, which as of 16:24 today has **672**
> unrecognised rows. **That test should be failing right now.** The implementing agent
> runs `cargo test -p oracle-core --test symbols_real_lst` first, before anything else in
> §8.

And one piece of prose is now **false**, four hours old — `symbols.rs:49`:

> *"The emitter supports `-` for equates, but **Aeon dumps no EQUs**, so all 2,129 rows of
> the real `s4.lst` are `C`"*

Aeon now dumps 670. (The clause it supports — that all *Symbol Table* rows are `C` — is
still true; the equates arrived in a section of their own, not in the type column.)

**Your design point is right and we are not going to decide it by accident.** Equates are
a genuinely different namespace: a value is not a location, and a same-named
equate/label collision must be resolved deliberately rather than by insertion order.
Ingesting them would change what `lookup_symbol` means and what an addr→name resolution can
return. That is a **ruled decision, not a parser change**, and it is registered as
`F-EQUATES-NAMESPACE` below. What we will do in the near slice is make the *current*
behaviour deliberate — recognise the section, skip it **without** counting it as damage,
and say so in the docstring — so that `is_intact()` stops being wrong about a healthy file
and so nobody later "fixes" the skip by folding equates into the symbol table.

### 6.4 Summary for your Build & Run decision

- **Sigil-built Aeon listings: work today, unchanged.** The new Equate Table costs us a
  now-wrong `is_intact()` and a red test, neither of which affects `load_symbols`.
- **Stock-S1 AS listings: refused today**, with
  `binding: "indeterminate-and-damaged"`, and the blocker is our AS-listing *reader*, not
  the ROM-binding check. Two changes make them viable and one of the two needs a ruling.
- **If you need something in the interim:** a listing with **no** `EndOfRom` symbol and no
  skipped rows is accepted unverified. A converter that emits our five-token one-per-line
  `Symbol Table` shape from an AS listing would be accepted that way today, with no server
  change at all. That is a workaround, not a recommendation — say the word and we will
  price the real fix instead.

---

## 7. Papercuts

**7.1 The `SUN_LEN` message (demand item 5).** `Server::bind` calls
`UnixListener::bind(path)` at `crates/oracle-aether/src/server.rs:344` and propagates the
`io::Error` unchanged; *"path must be shorter than SUN_LEN"* is Rust std's own text, from
`sockaddr_un` construction. The fix is a pre-bind length check in `bind`, before the
stale-socket probe at `:332`, naming the real limit — on Linux `sun_path` is 108 bytes
including the NUL, so **107 usable** — and naming the resolved path and the variable it
came from (`default_socket_path`, `:68-79`, is where `$ORACLE_SOCKET` wins). Message shape
to copy: the existing `AddrInUse` refusal at `:337-343`, which names the path.

**7.2 The events-without-`initialized` warning (demand item 7).** Feasible, one line, but
**not in `Session`** — that module is pure by design: *"Pure — no sockets, no engine, no
threads — so the handshake rules are unit-testable on their own"*
(`session.rs:1-6`). The state is already exposed: `wants_events()` (`:43-45`) and
`is_ready()` (`:38-41`). The site is the connection loop in `server.rs`, at the
`Ok(Action::Dispatch)` arm (`:572`), warning **once per connection** when
`session.wants_events() && !session.is_ready()`. Once-per-connection matters: a client in
this state will dispatch hundreds of methods, and a per-call warning is a log flood that
teaches the operator to filter it.

---

## 8. Slice plan

Sequential; each slice is tests-first and independently committable. **The implementation
is queued behind the profiler agent's slice — do not start until that agent's cargo is
idle.** A fresh agent can execute this without the recon session.

**Slice 0 — confirm the red test (do this first, it costs one command).**
`cargo test -p oracle-core --test symbols_real_lst`. §6.3 predicts
`real_s4_lst_parses_completely` fails on `skipped_lines() == 0` and `is_intact()`. If it
passes, §6.3's arithmetic is wrong and the rest of the equates work is re-scoped before
being written.

**Slice 1 — the Equate Table, made deliberate (no contract movement).**
*Tests first:* a fixture-shaped unit test in `symbols.rs` with a `Symbol Table` section
followed by an `Equate Table` section, asserting (i) the equate names resolve to **nothing**
through `by_name`/`by_demangled`/`resolve`, (ii) `skipped_lines() == 0`, (iii)
`is_intact() == true`. *Mutation requirement:* the test must fail if the section-skip is
deleted **and** if the skip is replaced by ingesting the rows as symbols — two negative
controls, because a single one passes for either mistake. Then: recognise the
`Equate Table` header, consume its rows and its `N equates` trailer without counting them
as damage; correct the now-false prose at `symbols.rs:49`; leave the equates
**unaddressable** (that is `F-EQUATES-NAMESPACE`). Re-green `symbols_real_lst.rs`.

**Slice 2 — CR-27 (docs only, in `empyrean`).** 27a, 27b, 27c per §2. Nothing in
`oracle-next` moves until it is adjudicated. The vendored schema re-vendor is part of
slice 3, not this one, because `the_vendored_schema_is_byte_identical_to_the_upstream_contract`
(`schema_conformance.rs:60`) would otherwise go red between the two.

**Slice 3 — serve `read_cram`.** *Tests first,* in a new
`crates/oracle-aether/tests/cram.rs`: the reply shape against the new fragment; `line`
out of range → `-32602` refused-not-clipped; the omitted-`line` shape; **not**
`require_paused` (assert it answers on a free-running machine); values agree with
`Engine::read` at `space:"cram"` for the same entry — a cross-instrument tie, the
`cram_rgb_matches_cram_decoded` precedent. *Mutation requirement:* an assertion that
still passes when the handler returns a fixed palette is vacuous — pin against a CRAM
state the test itself established. Re-vendor the schema, remove `write_cram` from the
harness's `schema_only` expectation as it becomes advertised, add the `METHODS` row
(32 → 33 advertised).

**Slice 4 — serve `write_cram`.** *Tests first:* `require_paused` → `-32005` on a
free-running machine (the `write_memory` test's shape); `r`/`g`/`b` and `raw` each land the
same colour; both together → `-32602`; `raw` out of 9-bit range → refused (not silently
masked — decide this in 27a and pin whichever way it rules); the reply's `cramAddr`/`value`
round-trip through `read_cram`; **the watch surface stays silent** — arm a `cram` watch,
poke, assert zero hits, which is the direct pin of §4.2's design choice and the one test
that would catch a later "simplification" into `write_target`. Then `Vdp::poke_cram` in
core with its own unit test against the `0x0EEE` mask and the `& 0x7E` layout.
33 → 34 advertised; the harness's `schema_only` list becomes empty and the CR-18-era
mismatch is closed.

**Slice 5 — the `write_memory` params policy.** Gated on 27c's ruling. *Tests first:*
`{symbol, offset: 2, …}` → `-32602` naming `offset`; `{symbol, disp: 2, …}` → writes at
`symbol+2` and the reply says so; the five known keys still pass; and — if 27c rules for
option (b) — one unknown-key test per method family. *Mutation requirement:* the rejection
test must name the offending key in the message, asserted on the string, or a blanket
"some params are invalid" refusal will satisfy it and help nobody.

**Slice 6 — the two papercuts** (§7). Small; can ride with slice 5.

**Slice 7 — hand the branch to Aurora.** They offered to point their probe at a branch
before it merges. Take it: their probe found item 3, which none of our 1,588 tests did.

---

## 9. Currency

**Expected movement: zero.** Currency here means the frozen golden hashes and the parity
corpus.

What could move, and why it will not:

- **`state_hash`'s `cram` component.** It hashes CRAM, and `write_cram` mutates CRAM — so
  the *capability* to move a currency exists. It cannot move one, because no frozen
  fixture calls `write_cram`: it is a brand-new method, reachable only by a client that
  names it, and every existing golden is produced by ROM execution. The same argument
  covered `write_memory` at CR-21.
- **The renderer.** `read_cram` reads `Vdp::cram()`, which is already read by
  `Engine::read`, `cram_decoded`, and the frontend swatch grid. No new read path, no
  mutation.
- **`Vdp::poke_cram`.** New, additive, and called from exactly one place. It does **not**
  touch `write_target`, so the guest-driven CRAM path — which every golden depends on — is
  byte-untouched. The mask/layout arithmetic is duplicated deliberately rather than
  factored, to avoid editing a function on the currency path; if the implementing agent
  prefers a shared helper, extracting one from `write_target` **is** a currency-path edit
  and must be its own commit with the goldens re-run.
- **The watch surface.** `poke_cram` does not `capture`, so no watch ring content changes
  and `watchpoint_hits` currencies stand.
- **The symbol parser (slice 1).** This one **does** touch a shared path and deserves the
  scrutiny. It changes only which lines are *counted as skipped* and adds a section the
  parser previously fell through; it adds no symbol and removes none. `symbols_real_lst.rs`
  goes from red to green, which is a correction, not a movement. No state hash, frame hash
  or parity-corpus entry reads a symbol table.
- **Advertised-method count.** 32 → 34. That is a wire-visible change and D5 makes it
  additive by construction; the harness's coverage pin (`UNCOVERED_METHODS`, empty) stays
  empty because both methods arrive with fragments.

---

## 10. Follow-ups registered

| id | what | revival condition |
|---|---|---|
| **F-LST-AS-COLUMNS** | Our `Symbol Table` row reader accepts one entry per line; AS emits two, costing 93–97% of a real AS listing's symbols (§6.2). Prototyped fix: split on `\|` before tokenising — 1,060 → 38,388 rows on `sonic3k.lst`. | A client needs AS-listing support. Aurora does, for classic-side Build & Run. |
| **F-LST-NONDEB2-BINDING** | No policy for a ROM with no `deb2` appendix. Stock S1/S2/S3K are all positively `Mismatch`ed today. **Needs a ruling** on what "bound" means for an image this project did not build. | Pairs with F-LST-AS-COLUMNS; neither alone makes stock-S1 viable. |
| **F-EQUATES-NAMESPACE** | Whether equates become addressable at all, and how a same-named equate/label collision resolves. Sigil's own position: values, not addresses, *"must never resolve as code/RAM addresses"*. **A ruled decision, not a parser change.** | A client asks to resolve a constant by name. |
| **F-PALETTE-DRAG-PACE** | Recorded, **not requested**. A `write_memory`-driven palette drag at 30 Hz = 30 pause/write/resume cycles/s = **60 stopped/resumed events/s** to every subscriber. Aurora's v1 is to throttle to ~10 Hz or coalesce-on-idle and measure. Two future shapes they named and did not ask for: relaxing the pause gate for small bounded writes, or apply-at-next-frame-boundary writes. | **Numbers.** A measured drag showing the event churn costs a subscriber real time, or a measured pause/resume cost that makes 10 Hz unusable. Not scheduled. |

**Not registered:** the whole-line batch `write_cram` (demand item 4) — **withdrawn by the
requester with a reason**, not deferred by us. §5.
