# CR-V, `state_hash` says in a typed key when the picture it fingerprints is not the one on screen, and two caveats that repeat a permanent fact on every reply move into the document

**Raised by:** oracle lane, 2026-09-12. Closes lens findings **M16** and **M18** from the 2026-09-06 sweep
(`docs/superpowers/notes/2026-09-06-oracle-lens-sweep.md:348-353`; triage rows `docs/2026-09-11-lens-triage.md:85`
and `:87`). Drafted by a research parcel that changed no code and ran no emulator.
**Target:** `contract/protocol.md` **§11.49** (next free: the last heading is §11.48, and no text anywhere cites
§11.49). **§6 amended in three places:** the `emulator/state_hash` row gains one result key, a new normative paragraph
follows the `framebufferSource` paragraph, and the `emulator/read` blockquote gains one bullet. Schema: one new
`$defs` entry, the `state_hash` fragment gains a key and a `dependentRequired`, and four description strings change.
**§2.4 is not amended**: its rules already require exactly this, and its advisory stays true. There is one optional
§8 item (30, next free) and one separable item (B) that amends the `screenshot` and `scanlines` rows.
**Reviewer:** to be named by the adjudicator in the ruling itself, per the 2026-08-27 substituted-reviewer rule
(`docs/OVERSEER.md`, "HUB RULING UNDER DELEGATION, 2026-08-27").

**Revisions everything below was read at.**
- **Contract:** empyrean **`origin/main` `61d378bce721efdb8c637adb06bd2988d6649166`**, printed by `git rev-parse`
  after `git fetch`, read only as `git show origin/main:<path>`. The brief's premises were measured at `e394fdb`;
  `git log e394fdb..origin/main -- contract/` is empty, so every contract line number the brief cites still holds.
- **Server:** oracle **`6b5b5c1`** (the branch base). Its only commit past `4a2865e` adds the dispatch brief, so
  every `crates/` line cited here is also the line at `4a2865e`.
- **Siblings swept**, each at its pushed ref: aeon `origin/master` `57528c222`, sigil `origin/master` `dd6e9f1b2`,
  aurora `origin/master` `b2116bc49`, seraph `origin/main` `991739898`, empyrean `origin/main` `61d378b`. Dominion
  and oracle-old have **no remote at all** (`git remote -v` prints nothing), so their committed `main` was swept
  instead: dominion `36f6068`, oracle-old `1eb09a9`. Oracle-old is reference-only as a server, but two pieces of it
  are **live** consumers (§4).

**How the letter was found.** Every `CR-<letter>` token in the contract at `61d378b`, headings and body, is one of
A to U, plus the named CRs (CR-BP, CR-SOCKET, CR-STEP-SHORTFALL). Every CR file under oracle's `docs/proposed/` is
F to R, and oracle's other docs use A to U. `git grep "CR-V\b"` finds nothing in empyrean, oracle, aeon, aurora or
sigil. The same grep form finds `CR-U` in the contract, so it can see a hit. **CR-V.**

---

## 0. What the adjudicator is asked to rule on

| Part | What it does | Can be taken alone? |
|---|---|---|
| **Core D1** (M16) | `state_hash` gains `displayMask`: the list of layers the display mask hides, present exactly when `framebuffer` is. | Needs D2, whose rule it reports. |
| **Core D2** (M16) | §6 says, for the first time, that `framebuffer` is always the **unmasked** picture. | Yes, as text. |
| **Core D3** (M18) | The permanent facts the two constant caveats carry move into §6. `state_hash`'s caveat becomes conditional, and `read_vram`'s stops being emitted. | Yes. |
| **Rider R1** | The schema enforces §6's existing sentence "`framebufferSource` is present if and only if `framebuffer` is". | Yes; droppable. |
| **Rider R2** | §6 pins which bytes `framebuffer` folds, so a client can fingerprint the picture it is looking at from `emulator/scanlines`. | Yes; droppable. |
| **Item B** (separable) | `displayMask` on `screenshot` and `scanlines` too, and `stateRender`'s definition names the mask. | Yes; the core does not depend on it. |
| **Booked C** (not proposed) | `pixel_attribution` and `object_at` also answer under the mask and disclose nothing. | Asks for a finding id. |

**No hash value moves under any part** (§5). No client must change in lockstep under any part. The only lockstep is
the reference server's own, landing in the same window as the re-vendor, which is the standing adoption condition.

## 1. Where the brief and the packet were wrong, first

1. **§11.27 does not make M18 a MUST NOT.** The packet says three methods emit an unconditional caveat "which
   §11.27 makes a MUST NOT". §11.27's sentence is about one method's caveat. It says (`protocol.md:4831-4833`) a
   server that cannot stamp per-entry writes *"MAY emit on any CRAM write since the line drew (coarser, still
   conditional) and MUST NOT emit unconditionally"*, and its subject is `pixel_attribution`'s caveat. The general
   rule is §2.4's advisory, which is a **SHOULD**: servers *"SHOULD prefer conditional caveats … and put a permanent
   property of a method in this document instead"* (`:630-632`). §11.48 M3 then put caveat presence outside the
   contract altogether: *"caveat presence and wording stay outside the contract (§2.4 rule 3)"* (`:5794-5795`).
   **So the reference server is conformant today on M18.** This CR acts on a SHOULD, and says so.
2. **M18 is two sites now, as the brief expected:** `read_vram` (`engine.rs:4870-4871`) and `state_hash`
   (`:5143-5145`). `read_memory` has been conditional since §11.48 (`:4464-4471`). Confirmed.
3. **The test comment at `crates/oracle-aether/tests/pixel_attribution.rs:732-738` is not stale in the way the
   brief says.** It was already rewritten: it now says `read_memory`'s caveat *"is conditional since
   F-DEBUGREAD-BANKED"* and that *"`read_vram`'s and `state_hash`'s constant caveats are still in the tree (lens
   M18)"*. It **will** go stale when this CR lands. It also says *"§11.27 raises it to a MUST NOT"* in a paragraph
   whose preceding sentence is about all three sites, which reads as the packet's over-broad claim. The parcel
   rescopes that sentence to this method's caveat (§8).
4. **Removing `read_vram`'s caveat would lose a fact the contract does not state anywhere.** Grepping
   `protocol.md` for `autoincrement`, `port path`, `FIFO`, `bypassing` or `debug read` finds no sentence saying that
   a VDP-space read comes straight from the array without touching the port. Worse, the published `emulator/read`
   fragment's `caveat` description says *"The debug-read property of these reads lives in the row's prose
   instead"*, and it does not. That is a dangling pointer. So D3's text is a precondition for retiring the caveat,
   not a nicety.
5. **The contract never says what the display mask does to any read.** Grepping `protocol.md` for `display mask`,
   `layer mask`, `LayerMask` or `masked` (as a layer word) finds one sentence, `:2134`, *"outside `state_hash`, as
   `LayerMask` is"*, about `screen_text`. So "`state_hash`'s framebuffer is unmasked" is today a server behaviour,
   not a contract rule. A typed key that reported it would be reporting a rule the document does not state, which is
   why D2 is part of the core.
6. **"Covers VDP state only" is an overclaim that should not be copied into the document.** The fragment's
   `$comment` and `memory_hash`'s prose both say it. But §8 item 29 (`:2518-2520`) records that the control-port
   write-pending toggle and the sprite-overflow and collision latches are VDP state that **no** fingerprint covers.
   Oracle's serve proved it: a status-port read cleared them and left every hash green. The new text names the four
   hashed regions exactly.
7. **`stateRender`'s definition is incomplete, which the packet did not see.** The `scanlines` and `screenshot`
   fragments define `stateRender` as *"the fallback when no completed frame is retained"*. Under a mask the server
   serves `stateRender` **with** a frame retained (`engine.rs:3657-3667`: a mask always takes the post-hoc path). This
   is the mirror of M16 on the sibling methods, so it is item B.
8. **Two more methods answer under the mask, and the packet did not list them:** `pixel_attribution`
   (`engine.rs:5029`) and `object_at` (`:6111`) both resolve through `pixel_attribution_masked(x, y, self.layers)`,
   and neither reply carries a word about it, not even a caveat. This is Booked C.
9. **Logistics.** The brief file was not in this worktree, which was cut at `4a2865e`. It is on `main` at `6b5b5c1`,
   so the branch was cut from there.
10. **The legacy server hashed a different picture layout**, which bears on R2. oracle-old
    `linux-port/gui/ControlSocket.cpp:2416` folds the frame's **RGBA** bytes, and oracle-rs folds **RGB**
    (`engine.rs:5153-5157`). The contract pins neither, so two servers' `framebuffer` values could never have agreed.

## 2. The evidence, at oracle `6b5b5c1`

**The hash is deliberately unmasked** (`crates/oracle-aether/src/engine.rs:5147-5157`):

```rust
if include_fb {
    // `LayerMask::ALL`, explicitly and always: this is a determinism fingerprint of what the
    // MACHINE drew. A display mask is the debugger's state, not the machine's, ...
    let (_, fb, from_raster) = self.framebuffer(LayerMask::ALL);
```

This is right and this CR keeps it: two identical machines must not disagree because one human hid a layer. The
tests pin it (`crates/oracle-aether/tests/layers.rs:722-789`, and `:876-885` asserts the digest does not move under
a mask).

**The only disclosure is prose appended to a caveat** (`:5174-5180`):

```rust
if let Some(extra) = self.masked_hash_caveat() {
    let base = out["caveat"]
        .as_str()
        .expect("state_hash always carries its own caveat")
        .to_string();
    out["caveat"] = json!(format!("{base} {extra}"));
}
```

`masked_hash_caveat` (`:3708-3721`) names the hidden layers and says the hash is of the UNMASKED picture. §2.4 rule 3
(`:614-618`) says a client *"MUST NOT parse it … Any consequence a client must act on needs its own typed key"*. A
client that hides plane A, screenshots, then hashes the framebuffer to pin what it is looking at has pinned a
different picture, and **no typed field in any reply tells it so**.

**Why the existing typed keys do not already say it.** Under a mask with a frame retained, `screenshot.source` is
`stateRender` and `state_hash.framebufferSource` is `raster`. A client *could* infer a mask from that mismatch. The
inference is indirect, depends on server internals, and **fails outright when no frame is retained**, where both say
`stateRender` and the pictures still differ. Calling `get_layer_states` beside the hash is not atomic either. The
player hosts this engine and its palette toggles move the same mask in-process between two bus requests
(`engine.rs:3679-3690`, *"there is no second mask anywhere"*). And even an exact read of the mask would not tell the
client that `state_hash` ignores it, because the contract does not say so (§1.5).

**The two constant caveats (M18):**

| Site | Text | The permanent property it carries | Where the contract states it today |
|---|---|---|---|
| `state_hash`, `engine.rs:5143-5145`, on **every** reply | *"these fingerprints cover VDP state only (VRAM, CRAM, VSRAM, VDP registers): they say nothing about the CPU, work RAM, the Z80, SRAM or audio. Two machines agreeing here can still differ."* | What the five hashes fold, and that agreement is not machine equality. | The fragment `$comment`, *"The five fingerprints cover VDP state only"* (an overclaim, §1.6); `memory_hash`'s prose `:1170`; §8 item 29 `:2519` in passing. **No §6 sentence on `state_hash` itself.** |
| `read_vram`, `engine.rs:4870-4871`, on **every** reply | *"debug read: taken straight from the VRAM array, bypassing the VDP port path, autoincrement, the FIFO and DMA."* | A VDP-space read is a peek of the array, not a port access. | **Nowhere** (§1.4). `read{space:"vram"}`, `read_vram`'s declared exact alias (`:1146-1148`), emits **no** caveat for the same bytes (`engine.rs:4605`, `:4616-4622`), so the two spellings already disagree about disclosure. |

## 3. The proposal

### 3.1 Core D1: `displayMask`

- **Name:** `displayMask`. **Type:** array of layer names from `set_layer_enabled`'s enum (`planeA`, `planeB`,
  `window`, `sprites`; §11.22 makes it `get_layer_states`' key set), `uniqueItems`, order not significant. `[]`
  when nothing is hidden. Defined **once** as `$defs/displayMask` and referenced by each method that carries it.
- **What it means:** the layers the debugger's display mask was hiding **when this reply was taken**. It describes
  the debugger, not a picture. On `state_hash` the method's own rule (D2) adds that `framebuffer` did **not** apply
  it, so a non-empty list means *"what `screenshot` and `scanlines` show you right now is not the picture this digest
  is of."*
- **Presence on `state_hash`:** present **if and only if `framebuffer` is**, which is the presence rule
  `framebufferSource` already has (`:2150`). It is absent when `includeFramebuffer` is false or omitted, because the
  mask cannot affect a reply that hashed no picture. It is `[]`, never absent, on an unmasked framebuffer reply.
  §2.3's reason governs here as it does for `get_layer_states`' four required keys: absence must not mean both
  "nothing hidden" and "this server predates the key".
- **Relation to `framebufferSource`:** independent, and a client needs both. `framebufferSource` says which
  *unmasked* picture was hashed (the retained raster or a state re-render). `displayMask` says whether the screen is
  showing something else. Under a mask the reference server's hash is typically `raster` while its screenshot is
  `stateRender`, and the key is what makes that disagreement explicit instead of inferred.

On the wire (envelope omitted):

```json
{"vram":"0x9F3B60476F9B240C","cram":"0x22B10C2223B16749","vsram":"0xB5F357FC0D28F314",
 "regs":"0x72065A81EA8200EA","combined":"0x5B0BDB191AC14796",
 "framebuffer":"0x74B7ECFAAF258A11","framebufferSource":"raster",
 "displayMask":["planeA","sprites"],
 "caveat":"A display layer mask is hiding planeA, sprites, and `framebuffer` deliberately fingerprints the UNMASKED picture, ..."}
```

**The schema change.** The keys and keywords below are exactly the ones patched into a scratch copy of the schema
and measured in §6. The description strings are the proposal; the measured patch carried the same sentences, and no
validator reads a description:

```json
"$defs": { "displayMask": {
  "type": "array", "items": {"enum": ["planeA","planeB","window","sprites"]}, "uniqueItems": true,
  "description": "The layers the debugger's display mask (emulator/set_layer_enabled) was hiding when this reply was taken: emulator/get_layer_states' key spellings, one entry per hidden layer, [] when none, order not significant. A fact about the DEBUGGER at reply time, not about any picture: each method that carries it says whether its picture applied the mask (§6, §11.49, CR-V). A server with no mask surface emits []." } }

"emulator/state_hash".result:
  "properties": { ...,
    "framebuffer": {"$ref": "#/$defs/hash64", "$comment": "Present if and only if the includeFramebuffer param was true. ALWAYS the unmasked picture: a server MUST NOT apply the display mask to it (§6, §11.49). displayMask says whether the picture on screen differs from it for that reason."},
    "displayMask": {"$ref": "#/$defs/displayMask", "description": "The display mask in force when `framebuffer` was taken, which `framebuffer` did NOT apply. Non-empty means emulator/screenshot and emulator/scanlines are showing a different picture from the one fingerprinted here. Present if and only if `framebuffer` is."},
    "caveat": {"type": "string", "description": "§2.4. Conditional: the reference server emits it only beside a non-empty `displayMask`, as that key's human-readable twin. Presence and wording are not contract (§2.4 rule 3); the permanent coverage property is in §6, not here."} },
  "dependentRequired": { "framebuffer": ["displayMask"], "displayMask": ["framebuffer"] }
```

`dependentRequired` already has precedent in this schema (`write_memory`, `write_cram`, `get_profiler_frames`).
The §6 row becomes: `` `framebuffer`?, `framebufferSource`?, `displayMask`? *(§11.49)* ``.

### 3.2 Core D2: the rule the key reports, and the coverage it replaces the caveat with (§6 text, drafted verbatim)

To follow the `framebufferSource` paragraph at `:2140-2151`:

> **What `state_hash` covers, and which picture `framebuffer` is** *(added §11.49, CR-V)*. The five fingerprints
> fold exactly four regions: VRAM (64 KiB), CRAM (128 bytes), VSRAM (80 bytes) and the 24 register bytes.
> `combined` is one stream over the four in that order. Nothing else enters them: not the 68000 or its work RAM,
> not the Z80, cartridge SRAM or audio, and not the VDP state that lives outside those four regions (the status
> word, the control-port write-pending toggle, the sprite-overflow and collision latches, the FIFO, DMA progress;
> §8 item 29 relies on this). **Two machines agreeing on all five can still differ.** A client that needs 68000
> memory compared uses `emulator/memory_hash`.
>
> **`framebuffer` is always the unmasked picture.** A server MUST NOT apply the display mask
> (`emulator/set_layer_enabled`) to it: it fingerprints what the machine drew, and a digest that moved because a
> debugger hid a layer would make two identical machines disagree for a reason that has nothing to do with either
> machine. The same reply carries **`displayMask`**, present if and only if `framebuffer` is: the layers the mask was
> hiding when the digest was taken, in `get_layer_states`' spellings, `[]` when none. A non-empty `displayMask`
> means `emulator/screenshot` and `emulator/scanlines` are showing a different picture from the one fingerprinted.
> A client that wants a fingerprint of the picture it is looking at does not get one from this method *(see R2)*.

**The caveat afterwards.** `state_hash` emits a caveat **only** beside a non-empty `displayMask`, and it is
`masked_hash_caveat`'s sentence standing alone, with no constant prefix. It is the human twin of the key: §2.4 rule 2
says clients SHOULD surface caveats to a person, and a person reading a harness log should see *"plane A is hidden;
this digest is of the unmasked picture"* without decoding a list. With no framebuffer, or with no mask, there is no
caveat. The fragment keeps `caveat` declared (§2.4 rule 4), and its description string changes as in §3.1.

### 3.3 Core D3 (M18): `read_vram`

To be added as a sixth bullet of the `emulator/read` blockquote (`:1124-1144`). It belongs there because `read_vram`
is declared an exact alias of `read{space:"vram"}` (`:1146-1148`), and the property is true of all three VDP spaces:

> - **A VDP-space read is a peek of the chip's memory, not a port access** *(added §11.49, CR-V)*. For `vram`,
>   `cram` and `vsram`, and for the deprecated `emulator/read_vram`, the bytes are taken straight from the array: no
>   VDP data or control port is used, so there is no autoincrement, no FIFO and no DMA interaction, and the read
>   leaves the port state exactly as it found it. This is permanent and is not repeated per reply.

**After the change** `read_vram` emits no caveat on the reference server. The fragment **keeps `caveat` declared**
(over-declaring is harmless, §11.20), so a genuinely conditional one needs no amendment later. A plausible example
is *"a VRAM fill or copy DMA was in progress when this was read"*, which can happen at a breakpoint stop, but it is
not proposed here. Two description strings change so the pointer at §1.4 stops dangling: the `read_vram` fragment
`$comment` (text in the §6 patch) and nothing on `read`, whose description becomes true once the bullet lands.

### 3.4 The better-approach pass: every alternative, and why each lost

| Alternative | Why it lost |
|---|---|
| **A boolean** (`displayMasked: true`) | It cannot be printed as *which* layers, which a display consumer needs, and it cannot be compared against `get_layer_states`. The list gives the boolean for free as `length > 0`. |
| **`hiddenLayers`**, the obvious name | **On `state_hash` it reads backwards.** `hiddenLayers: ["planeA"]` beside a digest says "the hashed picture hid plane A", the exact opposite of the truth. One name meaning "hidden from this picture" on `screenshot` and "hidden, but not from this picture" on `state_hash` is the *two vocabularies wearing one name* shape `:1133-1134` names as a defect. `displayMask` names the debugger's state, which is true on every method; each method's rule then says whether its picture applied it. |
| **An object mirroring `get_layer_states`** (`{planeA:true,…}`) | It reuses a known shape, but `true` means *shown*, so every client inverts it. It puts four keys on every framebuffer reply for the common case of nothing hidden. And it grows a required key per new layer where the list grows only an enum value. |
| **Absent when nothing is hidden** | This keeps an unmasked reply byte-identical (the property `masked_hash_caveat`'s doc values), but absence then means both "no mask" and "server predates the key". That is §2.3's failure; `get_layer_states` requires all four keys for exactly this reason. The price of the chosen rule is one `"displayMask":[]` on unmasked framebuffer replies, and the sweep (§4) found no consumer that could see it. |
| **Always present, even without `framebuffer`** | The mask cannot affect a reply that hashed no picture. A mask-dependent key there is noise in typed form, and case 6 in §6 pins its absence. |
| **Also offer a masked hash** (`framebufferMasked`) | Rejected, for four reasons. **(1)** It would put debugger state into the method whose whole contract is machine determinism, where a gate could pick it up by mistake. **(2)** It costs a second full render per call whenever a mask is set. **(3)** It is already obtainable. `emulator/scanlines` returns the masked picture's RGB rows, and FNV-1a-64 over them decoded and concatenated is, on the reference server, exactly the fold `framebuffer` performs (`engine.rs:5153-5157` against `:5787-5797`: the same pixels, the same `r,g,b` order). **(4)** What is actually missing is the contract promise that those bytes are the ones `framebuffer` folds, which is R2, a text-only rider. |
| **Keep the caveat as the carrier but give it structure** | §2.4 declares `caveat` a string, and CR-G's own red vector refuses a structured one. That is a typed key with worse manners. |
| **The client calls `get_layer_states` around the hash** | Not atomic: the player's palette moves the same mask in-process between two requests (§2). And without D2, knowing the mask does not tell the client the hash ignores it. |
| **Infer it from `framebufferSource` against `screenshot.source`** | Indirect, and it fails when no frame is retained (§2). |
| **Pin "masked implies `stateRender`" in the schema** (item B) | True on this server, but it would outlaw a *better* one: a server that retained per-line layer buffers could mask a raster-timed frame, and that would recover the mid-frame effects today's masked picture loses. The schema does not enforce the tie, and item B's prose says MAY. |

### 3.5 Riders (each droppable)

**R1, enforce an existing MUST.** §6 already says *"`framebufferSource` is present if and only if `framebuffer`
is"* (`:2150-2151`), and nothing checks it. The rider extends D1's `dependentRequired` to
`{"framebuffer": ["framebufferSource","displayMask"], "framebufferSource": ["framebuffer"], "displayMask": ["framebuffer"]}`.
The reference server already obeys it (`engine.rs:5157-5161` sets both together), so no reply changes. It is in
this CR only because D1 defines its presence rule relative to the same pair, and one `dependentRequired` reads
better than two half-rules.

**R2, pin `framebuffer`'s input bytes.** To be appended to D2's paragraph:

> The digest folds the active display's pixels, line 0 to line 223, each line left to right, three bytes per pixel
> in `r`, `g`, `b` order: exactly the bytes of `emulator/scanlines`' `rows[].rgb` for lines 0-223, decoded and
> concatenated. So a client can fingerprint the picture it is looking at, masked or not, with one `scanlines` call
> and the FNV-1a-64 parameters `emulator/memory_hash` states.

This is what makes the "why not offer a masked hash" answer a contract promise rather than an accident of one server.
It moves **no** oracle-rs value: this is the layout oracle-rs folds today. It does make the legacy C++ server's
RGBA fold (`ControlSocket.cpp:2416`) non-conformant, which is already true of that server on the stamp (§8's *"The
stamp's cost, stated plainly"*).

### 3.6 Separable item B: the same key on `screenshot` and `scanlines`

This is the mirror half, and arguably the more fundamental one: it is the *picture* that is not the machine's.
Proposed:

- both results gain **`displayMask`** (`$ref` to the same `$defs` entry), **always present**, meaning *"the layers
  hidden from this picture"*;
- both fragments' `source` descriptions gain one sentence: *"A non-empty `displayMask` is a second reason for
  `stateRender`: the reference server's retained frame is drawn unmasked, so it re-derives a masked picture from
  current VDP state. A server able to mask a raster-timed frame MAY answer `raster` under a mask."* The §6
  `framebufferSource`/`source` paragraph (`:2140-2151`) gains the same clause. The wording is permissive on
  purpose: the third-last row of §3.4 explains why "masked implies `stateRender`" is not made a rule.

A client then has both halves as typed fields: a picture with a non-empty `displayMask` never matches a
`framebuffer` digest, and a `stateRender` with an empty one means no frame was retained. Aeon's six gates that
refuse non-`raster` scanlines (§4) already refuse masked readbacks correctly; item B gives their error message a
typed reason to print. **The core does not depend on B.**

### 3.7 Booked, not proposed: C

`emulator/pixel_attribution` (`engine.rs:5029`) and `emulator/object_at` (`:6111`) resolve the winner through
`pixel_attribution_masked(x, y, self.layers)`. With plane A hidden, a bus client asking "what is at (x, y)" is told
plane B, with nothing in the reply or the contract saying the answer depends on a debugger toggle. For the player's
click panel that is the right answer, since it resolves the dot a person sees. For a scripted client it is the same
silent divergence as M16. It is **not** folded in here because it needs its own consumer sweep over two more
methods, and one of them (`object_at`) has a sibling consumer shape this parcel did not enumerate. **Ask:** the
adjudicator books it as a finding (a suggested name is F-MASKED-ATTRIBUTION), whose natural fix is the same
`displayMask` key.

### 3.8 Optional §8 item 30: the mask never reaches the hash

> 30. **`state_hash`'s framebuffer is mask-blind** *(§11.49, CR-V)*. With any display mask set, `framebuffer` and
>     `framebufferSource` are byte-identical to the reply at the same machine point with no mask, and `displayMask`
>     names exactly the hidden layers. Game-agnostic recipe: on a paused machine, hash with `includeFramebuffer`,
>     hide one layer with `set_layer_enabled`, hash again, restore, hash a third time; all three digests equal, and
>     the second reply's `displayMask` is that one layer. *Anti-vacuity:* between the first two hashes,
>     `emulator/scanlines` must differ, or the fixture draws nothing on the hidden layer and the row proves nothing.

It is optional because oracle's own tests already pin it (`tests/layers.rs:722-789`). It is offered because
aurora's conformance harness certified §11.41 from outside the server, and this recipe is the one it could run.

## 4. The consumer sweep (shared review bar 14, three bins)

**Method.** Each sibling was grepped at its pushed ref with `git -C <repo> grep <pattern> <ref>`, never its
working tree, with worktree copies and vendored trees excluded by using the ref. Bar 14 says an identifier grep and
a quoted-key grep are different questions, so both spellings were run together:

- **P1**, method and field names, both spellings: `state_hash|stateHash|state-hash|emulator_state_hash|emulator_read_vram|read_vram|readVram|framebufferSource|includeFramebuffer|get_layer_states|set_layer_enabled`,
  plus the caveats' own words, `fingerprints cover VDP|debug read: taken|UNMASKED`.
- **P2**, `caveat`: generic display and pass-through code that shows any reply's caveat.
- **P3**, the proposed key and the names it beat: `displayMask|hiddenLayers|maskedLayers`.

Code hits were read in context; documentation hits were counted and not treated as consumers.

**Positive controls.** For P3 (zero everywhere), the same command form with `framebufferSource` finds 7 hits in
empyrean (`contract/protocol.md` 6, the schema 1). For seraph, where P1 and P3 are both zero, the same command form with
`emulator/|oracle|caveat` finds 65 lines across 13 files (docs, theme tokens), so the grep reads the tree. For dominion, P2 finds 6 lines of its own
prose (`server/src/usage.ts:45`, and others). For the CR letter, `CR-U\b` finds the contract (§0).

| Repo @ ref | BRANCH (decides on the value) | PASS-THROUGH (moves the value on) | DISPLAY (shows it to a person or a model) |
|---|---|---|---|
| **aeon** @ `57528c222` | **none** on either caveat. Its scanlines gates branch on the typed `source` (below). | `read_vram` callers in 13 tools (e.g. `tools/waterline_art_witness.py:131-137`, `tools/dplc_coherence_witness.py:557-558`) read `bytes` only. P2 over all 13 finds only scanlines or `run_to` caveats and prose, never `read_vram`'s. `state_hash` is reached only through oracle-old's `ab_runner.py` (row below). | Six tools print a **scanlines** caveat when `source != "raster"`: `boot_override_gate.py:374-376`, `curve_desc_probe.py:159-161`, `sec7_waterline_probe.py:133-135`, `vsplit_landing_gate.py:205-207`, `warp_mailbox_gate.py:239-241`, `hblank_window_sweep.py:413-415`. Neither the core nor B changes a scanlines caveat. Under B they gain a typed reason they could print. |
| **sigil** @ `dd6e9f1b2` | none | Nine A/B scripts under `crates/sigil-harness/golden/ab/` call `state_hash` (`includeFramebuffer: true` in seven) and **copy named keys only**: `vram, cram, vsram, regs, combined`, `framebuffer` (e.g. `g9/ab_g9_state.py:76-79`, `wavec/ab_wavec_state.py:71-74`). The committed manifests hold those keys and no caveat (P2 finds no caveat in `golden/` code). **Unaffected.** | none |
| **aurora** @ `b2116bc49` | none | `scratchpad/vdp-registers-peek-harness.mjs:580-626` compares five named fields. **`scratchpad/s1-vplayer-spike-probe.mjs:184-185` stringifies the WHOLE reply** (`h?.hash ?? JSON.stringify(h)`, and `hash` does not exist, so the whole object including `caveat` and the stamp) and compares two runs made in one process against one server. Removing the caveat changes both strings together, and it asks for no framebuffer, so `displayMask` never appears. **Unaffected, and the only whole-reply consumer found.** A variant that compared against a string stored from an older build would see the caveat's removal as a difference. | none. P2's `src/` hits are aurora's own caveats (`ramp-sign-lag.ts`, `conflict-message.ts`), not bus replies. |
| **seraph** @ `991739898` | none | none | none (no bus client) |
| **empyrean** @ `61d378b` | none | none | The contract itself is the subject; `contract/schema/tests/vectors.json` has **no** `state_hash` or `read_vram` case (counted: zero). |
| **dominion** (no remote) @ `36f6068` | none | none | none. P1 and P3 are zero; its P2 hits are its own fields. |
| **oracle-old** (no remote) @ `1eb09a9`, reference-only as a server | none | **LIVE:** `linux-port/harness/ab_runner.py:205-208`, which aeon's `tools/effects_gates.py:826,888` runs for every committed scene. It copies `combined, vram, cram, vsram, regs` only: **unaffected**. `determinism_gate.py:38` reads `combined`: unaffected. | **LIVE: the `oracle` MCP server registered in this machine's Claude config is `oracle-old/linux-port/mcp/oracle-mcp`.** `_pretty` (`oracle_mcp.py:1259-1261`) sends every result to the model as `json.dumps(result)`. Today the model reads the "VDP state only" sentence on every `emulator_state_hash` call and the debug-read sentence on every `emulator_read_vram` call. After D3 it reads neither, and its only source for the coverage fact becomes the tool description at `:496-502`, which says *"Hash the full VDP machine state"*, an overclaim (§1.6). **Not lockstep, but a follow-up is owed** (§8). After D1 the model sees `displayMask`, which is additive. |
| **oracle** (this repo) @ `6b5b5c1` | **Tests that pin the caveats.** `tests/methods.rs:225-227` asserts the text contains `"VDP state only"`. `tests/layers.rs:799-817` (`.expect("emulator/state_hash always carries a caveat")`), `:872-874`, `:887-917` (the masked caveat *strictly extends* the constant) and `:950-985` (the no-op and no-framebuffer cases compare `caveat`). `engine.rs:5175-5177` `.expect`s the constant. All change in the implementing parcel. | The in-process player never calls either method over the bus. The Memory panel reads VRAM through the engine's free function (`crates/oracle-player/src/memory.rs:164-168`) and only **names** `emulator/read_vram` as the matching method (`:101-102`). `tools/aether_smoke.py:183-185,241-242` reads named keys. | `crates/oracle-player/src/ui.rs:613-625` draws the standing mask statement from `LayerMask::hidden()` in-process, the same derivation `displayMask` reads (`engine.rs:3675-3677`), so the two cannot disagree (§7). Doc comments quote the constant: `tests/pacing.rs:657-660` (goes stale) and `tests/pixel_attribution.rs:732-738` (goes stale, §1.3). Dated docs that quote it (`docs/2026-08-14-aether-change-requests.md:986`, `docs/2026-09-11-debugread-banked.md:174`, `docs/OVERSEER-LOG.md:592`) are records and stay. |

**P3 (the new key): zero hits in every repo.** Nothing anywhere reads, shows or passes on `displayMask`,
`hiddenLayers` or `maskedLayers` today.

## 5. Classification (bar 14's corollary: classify before pricing)

| Wire-visible change | Replies affected | Lockstep? |
|---|---|---|
| `displayMask` **added** (D1) | every `state_hash` reply with `includeFramebuffer: true`. It is `[]` when nothing is hidden, so this is the one byte change an unmasked reply sees. | **Additive for every client** (no consumer found that could see it; §4). **Lockstep for the reference server only**: today's framebuffer reply is vector 5's red once the re-vendor lands, so serve and re-vendor land in one window. |
| constant `caveat` **removed** from `state_hash` (D3) | every `state_hash` reply with no mask set, which is almost all of them | No client branches on it. The live MCP bridge shows it to a model: the loss is covered by D2's §6 text plus the owed tool-description fix. Oracle's own tests change in the parcel. |
| `state_hash` masked `caveat` **text changed** (D3) | framebuffer replies under a mask: the constant prefix goes, and the masked sentence stands alone | Caveat wording is not contract (§2.4 rule 3). Oracle's `layers.rs` strict-extension assertion is rewritten against the typed key. |
| `read_vram` `caveat` **removed** (D3) | every `read_vram` reply | No consumer reads it (§4). This brings `read_vram` into line with its alias `read{space:"vram"}`, which never carried it. |
| schema descriptions reworded; `$defs/displayMask`; `dependentRequired` (D1, R1) | none (documents only) | none |
| `displayMask` **added** to `screenshot`/`scanlines` (B) | every reply of both | Additive for clients; server lockstep as above. |

**No hash value moves**, under any part. D1 and B add a sibling key and change no computation. The FNV-1a layout
of the five fingerprints is the suite contract in `crates/oracle-core/src/state_hash.rs` (*"Do not change the byte
order, the masking, the region sizes, or the output format"*), and nothing here touches it. `framebuffer` keeps
being computed at `LayerMask::ALL` (`engine.rs:5152`), which is the rule D2 writes down. R2 pins the layout
oracle-rs already folds.

## 6. Vectors, with the verdicts measured

`docs/proposed/2026-09-12-cr-v-vectors.json`, 18 cases in the CR-G shape. **How they were run:** the contract's
`bus-protocol.schema.json` and `tests/validate_contract_schema.py` were taken from empyrean `61d378b` by `git show`
into a scratch tree, and the upstream baseline was run first. Three patched variants were then built: **core**
(D1), **core+R1**, and **full** (core+R1+B). Each case was judged against all four schemas with the validator's own
`errors_for`, open (G3) and, for passing documents, closed (G4, §8 item 20's closure). The upstream validator was
then run whole on each variant with that variant's cases appended to the upstream vectors.

**Whole-gate results:** upstream baseline **GREEN** (119 pass, 176 red, 70 closure); core **GREEN** (123 / 182 / 74);
core+R1 **GREEN** (123 / 184 / 74); full **GREEN** (126 / 187 / 77). G1 checks 148 fragments.

| # | Case | Expect | upstream | core | core+R1 | full |
|---|---|---|---|---|---|---|
| 1 | `state_hash`, framebuffer, `displayMask: []` | pass | open pass, **closure RED** (`displayMask` undeclared) | pass | pass | pass |
| 2 | masked, `["planeA","sprites"]` + caveat | pass | closure RED | pass | pass | pass |
| 3 | no framebuffer, no key, no caveat (the post-M18 default) | pass | pass | pass | pass | pass |
| 4 | masked, `stateRender`, no caveat | pass | closure RED | pass | pass | pass |
| 5 | framebuffer, no `displayMask` (**today's reply**) | fail | pass | **RED** (`'displayMask' is a dependency of 'framebuffer'`) | RED | RED |
| 6 | `displayMask` without framebuffer | fail | open pass | **RED** (`'framebuffer' is a dependency of 'displayMask'`) | RED | RED |
| 7 | `["backdrop"]` | fail | open pass | **RED** (not one of the enum) | RED | RED |
| 8 | `["planeA","planeA"]` | fail | open pass | **RED** (non-unique elements) | RED | RED |
| 9 | `true` | fail | open pass | **RED** (not of type array) | RED | RED |
| 10 | `"planeA, sprites"` | fail | open pass | **RED** (not of type array) | RED | RED |
| 11 | R1: `framebufferSource` without framebuffer | fail | pass | **pass** | **RED** | RED |
| 12 | R1: framebuffer + key, no `framebufferSource` | fail | open pass | **pass** | **RED** | RED |
| 13 | B: `screenshot`, `[]`, raster | pass | closure RED | closure RED | closure RED | pass |
| 14 | B: `screenshot`, masked, stateRender + caveat | pass | closure RED | closure RED | closure RED | pass |
| 15 | B: `screenshot` without key (today's reply) | fail | pass | pass | pass | **RED** (required) |
| 16 | B: `scanlines`, masked, one H40 row | pass | closure RED | closure RED | closure RED | pass |
| 17 | B: `scanlines` without key (today's reply) | fail | pass | pass | pass | **RED** (required) |
| 18 | B: `scanlines`, `["backdrop"]` | fail | open pass | open pass | open pass | **RED** (enum) |

**What the columns prove.** Every red is accepted by the upstream open fragment, which is what G3 judges a red
against. So each red verdict is produced by this CR's patch and not by something already in the schema. R1's and
B's reds pass without their own part, which is why each case is tagged and can be dropped with its part. The
passing documents that carry the key fail upstream closure, so the patch is what admits the key.

**What is deliberately NOT a vector** (§11.27's correction paragraph is the precedent: two proposed vectors there
were not expressible as schema documents):
- "the caveat is present exactly when `displayMask` is non-empty": case 4 is a valid document by design, so it is
  row CR3 below;
- "`framebuffer` does not move under a mask": the same digest in two documents is invisible to a schema (CR2);
- "`displayMask` names exactly the hidden layers": a schema cannot see the mask (CR2);
- **anything for M18.** Both fragments keep `caveat` declared, so a reply with the old caveat and one without it
  are both valid. Emission is server behaviour (CR4, CR5).

## 7. Conformance rows for the implementing parcel (live, on oracle's wire)

- **CR1**, unmasked. `state_hash {includeFramebuffer: true}` carries `displayMask: []` and **no** `caveat`.
- **CR2**, masked. With `planeA` and `sprites` hidden, `displayMask` is set-equal to `{planeA, sprites}` and names
  neither `planeB` nor `window`. `framebuffer` and `framebufferSource` are byte-identical to CR1's reply at the same
  machine point. *Anti-vacuity*, kept from today's test: the mask must be shown to reach the picture (a
  `screenshot` answering `stateRender`, or `scanlines` rows differing) before the hash's sameness means anything.
- **CR3**, informative, the reference server's behaviour and not contract. A caveat is present in CR2's reply and
  names the hidden layers, and it is absent in CR1's. The two differ in the mask and nothing else.
- **CR4**, M18 on `state_hash`. `state_hash {}` with or without a mask has no `caveat` and no `displayMask`, and its
  keys are exactly the five fingerprints plus the envelope.
- **CR5**, M18 on `read_vram`. An in-range `read_vram` has no `caveat`, and its `bytes` equal `read{space:"vram"}`'s
  for the same range.
- **CR6**, vocabulary. `LayerMask::targets()`' names equal `$defs/displayMask.items.enum` in both directions: extend
  `tests/layers.rs::the_mask_vocabulary_is_the_contract_fragments_own`.
- **CR7**, player parity. With a mask set from the palette (`Engine::set_layer`), the wire's `displayMask` equals
  `LayerMask::hidden()`, the badge's source. It is one derivation, asserted so it stays one.
- **CR8**, if R2 is taken. With no mask and both sources `raster`, FNV-1a-64 over `scanlines` rows 0-223 decoded and
  concatenated equals `framebuffer`. Under a mask that hides drawn content, the same fold over the masked rows
  differs from `framebuffer`: that is the anti-vacuity half.
- **CR9**, if B is taken. `screenshot` and `scanlines` carry the same `displayMask` as `state_hash` at the same
  point, `[]` when unmasked.

## 8. Implementation sketch (a map, not a patch)

- **`crates/oracle-aether/src/engine.rs`.** In `state_hash` (`:5125-5183`), delete the constant caveat
  (`:5143-5145`). When `include_fb` is set, write `out["displayMask"] = json!(self.masked_layer_names())`, and when
  that list is non-empty write `out["caveat"]` from `masked_hash_caveat()`, reworded to stand alone. The `.expect`
  append block (`:5162-5180`) goes. Reword `masked_hash_caveat`'s doc (`:3699-3707`): its claim that "no mask keeps
  the unmasked reply byte-identical" becomes the D1 rule. In `read_vram` (`:4866-4872`), drop the caveat. For B:
  `screenshot` (`:6947-6966`) and `scanlines` (`:5800-5822`) gain the same key.
- **Re-vendor** `crates/oracle-aether/tests/contract/bus-protocol.schema.json`, `vectors.json` and `PROVENANCE.md`
  from the ruling commit. `tests/schema_conformance.rs:300` runs the vendored vectors, and item 20's closure will
  refuse the new key until the fragment declares it, so the serve and the re-vendor land together.
- **Tests.** `tests/methods.rs:225-227` flips to "no caveat" and gains CR5. `tests/layers.rs:791-986` is rewritten
  around the typed key (CR1-CR4): the helper `unmasked_state_hash_caveat` expects a caveat that no longer exists,
  and the strict-extension assertion has no base to extend. Keep every hash-does-not-move assertion and the
  anti-vacuity precondition. Add CR6 and CR7, and CR8 and CR9 for the riders.
- **Stale comments.** `tests/pixel_attribution.rs:732-738`: drop "still in the tree (lens M18)", and rescope "§11.27
  raises it to a MUST NOT" to this method's caveat. `tests/pacing.rs:657-660`: point at the §6 coverage paragraph
  instead of "its own reply says so in a caveat". `engine.rs:4464-4468` stays true.
- **Follow-up, not lockstep:** the MCP bridge's `state_hash` tool description (oracle-old
  `linux-port/mcp/oracle_mcp.py:496-502`) should drop "full VDP machine state", state the four regions, and mention
  `displayMask`.

## 9. What would have to be true for this to be wrong

- **A consumer outside the seven swept trees stores a whole `state_hash` reply and compares it across server
  builds.** The caveat's removal would then read as a difference. The sweep found one whole-reply consumer (aurora's
  spike probe), and it compares within one run. Nightly checkouts or out-of-tree scripts were not visible to this
  sweep. Bar 14's precedent is exactly such a checkout.
- **The mask is meant to reach the hash.** Then D2 is the wrong rule, and the fix is a different CR. Nothing found
  argues for it: oracle's code, its tests and its commentary all say the opposite.
- **`displayMask` should exist even without a framebuffer.** That would be the case if some client needs the mask
  in every hash reply for a reason unrelated to the picture. None was found, and `get_layer_states` answers it
  atomically enough for that purpose.
- **R2's layout claim is wrong in a corner.** For example, a mid-frame width switch could leave `scanlines` rows at a
  normalized width the hash does not fold the same way. Both read the same `framebuffer()` output on oracle-rs
  (`engine.rs:5152`, `:5775`), so this is expected to hold. CR8 is the row that would catch it, and R2 is droppable
  if it does not.
