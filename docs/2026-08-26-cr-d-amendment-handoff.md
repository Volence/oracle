# CR-D, adjudicated and applied: the empyrean amendment, drafted and ready to land (2026-08-26)

**Pairing.** This is the oracle half of a two-repo parcel, and it is the half that **does not land**. The
contract text and schema below belong in `empyrean/contract/`, and only the empyrean lane commits them.
This document is the finished text, ready to be applied without further drafting.

| | |
|---|---|
| **Written on** | oracle branch **`cr-d-apply`**, cut from oracle `main` **`ed05ff0`**; **delta ruling applied** on branch **`cr-d-delta-apply`** |
| **Sources** | the CR `docs/2026-08-26-cr-d-object-decoders.md` (as amended by this parcel's first commit), the ruling `docs/2026-08-26-ruling-cr-d.md` — verdict **ADOPT WITH CHANGES** — and the delta ruling `docs/2026-08-26-ruling-cr-d-delta.md`, which amended two of the three items flagged at §9 (**M5** `role` survives inactivity, **M7** `object_slot` takes the M2 conditional) and upheld the third (**M6**) |
| **Hold** | **RELEASED.** The delta held this handoff on M7 and said it releases *"by application, not by a further delta"*, on the condition that M5 and M7 are applied, §8's validation re-run over the amended fragments with the four new vectors, and its output quoted here. All three are done and §8's run is **ALL GREEN** |
| **Targets** | empyrean `contract/protocol.md`, `contract/schema/bus-protocol.schema.json`, `contract/schema/tests/vectors.json` |
| **Anchored at** | empyrean `origin/main` **`78d432235090ae53848f4f6725f36ac148ff1ef4`** — read after `git fetch`, through `git show <rev>:<path>`, never through the sibling directory path |
| **Contract blobs there** | `protocol.md` **`b4776ce90000a89ee50755892f999c03e5130e99`** (4265 lines) · schema **`7b24bcedc24f0a6aa7dd4504f4e2f9bf63e4cda7`** |
| **Re-verified at delta-apply time** | empyrean `origin/main` has moved on again, to **`4b2f3dd8462ea3f5edb1d43fddd438762a60ff8c`** — and the contract has **not** moved with it: `protocol.md` **`b4776ce90000a89ee50755892f999c03e5130e99`**, schema **`7b24bcedc24f0a6aa7dd4504f4e2f9bf63e4cda7`**, `vectors.json` **`a5051ba2b5057e8058df92498a2a60492807a775`**, every one byte-identical to its blob at the anchor `78d4322`. The single commit touching `contract/` in between (`db13224`) edits `contract/LANE_STATUS.md` alone. **§11.25 is still free**: `protocol.md` at the tip still ends at line **4265**, its last section is still **§11.24** at `:4227`, and `grep -c '^### 11.25'` returns **0** |
| **Runtime** | none. No `cargo`, no emulator MCP tool, no server started. Nothing committed to any `main`. `docs/lane-status.json` untouched. |

**Did the tip move, and did the contract move with it?** The tip **moved** — the CR anchored at `39cfaa27`
and `origin/main` is now `78d4322`. **The two contract files did not.** Both blob ids are byte-identical at
`39cfaa27` and `78d4322`, and the only commit touching `contract/` in between (`209c7fe`) edits
`contract/projects.json` alone. So this amendment is drafted against exactly the artifact the CR read and
the ruling adjudicated — checked rather than assumed, by `git log 39cfaa27..origin/main -- contract/` and
by comparing `git rev-parse <rev>:<path>` at both revisions.

**The section number is §11.25, and it is free at the tip, not at the CR's anchor.** `protocol.md` at
`78d4322` ends at line 4265; the last amendment section is **§11.24** (*batch B1, nine small run-control and
read defects*) beginning at `:4227`. Nothing has been appended since. **§11.25 is therefore the next free
number** — re-derived here at the tip rather than carried from the CR, because the CR-C parcel's own record
shows what happens otherwise: its ruling said "add a §11.21" and by landing time §11.21 and §11.22 had both
been taken, so the entry became §11.23. If another amendment lands before this one, renumber and update
every cross-reference; the strings to change are listed in §7 below.

---

## 1. Every delta, in one table

| # | Target | Delta | Driven by |
|---|---|---|---|
| **A** | `protocol.md:1492-1503` | Replace the `### object / player decoders ⚙` group — three rows rewritten, `call_stack` untouched, and the ⚙ note replaced with four normative rules | CR §10.1, plus S3's D9 sentence and S4's *at least one* pin |
| **B** | `protocol.md:2142` (§8 item 20) | **Append one sentence** pinning the closure scope to the top level of the result | **M4** (Q6's pin made durable) |
| **C** | `protocol.md:2277-2278` (§9) | Append the partial-lift paragraph to the Phase-5 decoder deferral | CR §10.2 |
| **D** | `protocol.md:4266+` | **New §11.25**, appended after §11.24 | CR §10.4, extended by the six things §10.4 requires it to record |
| **E** | schema, top-level `description` | **EIGHT → FIVE** BLOCKED rows; the three departures pointed at §11.25; the gate's derived list replaces *"prints all eight"* | **M1(i)** |
| **F** | schema `$defs` | Two new: `decoderLayout` (closed) and `decodedSlot` (**unclosed, no `required`**) | CR §10.5, refactored by **M2** and **M3** |
| **G** | schema `methods` | Three new fragments: `emulator/object_list`, `emulator/player_state`, `emulator/object_slot` | D2, D3, D5 — Q8 answered *travel* |
| **H** | schema `capabilities.objectDecoders` | A `description` carrying §8.1's normative sentence. Type unchanged | D4 + **S4** |
| **I** | schema `limits` | One new **OPTIONAL** key, `maxObjectSlots` | CR §10.5 |
| **J** | `vectors.json` | **22 new cases**: 9 accepting, 13 refusing — counts re-derived from §8's run, never carried | **M1(iii)**, extended by the delta ruling's **M5** and **M7** (cases 19–22) |

Nothing else in either artifact is touched. No method is removed, no key is removed, no published type
changes.

---

## 2. Delta A — the replacement §6 group

Replace `protocol.md:1492-1503` in full (the heading, the four rows, and the ⚙ note) with:

```markdown
### object / player decoders ⚙
| Method | params | result |
|---|---|---|
| `emulator/object_slot` ⚙ | `slot`, `fields`?, `includeBytes`? | `layout`, `active`, the item keys hoisted, `caveat`? |
| `emulator/object_list` ⚙ | `limit`?, `fields`?, `includeBytes`? | `objects[]`, `total`, `returned`, `limit`, `truncated`, `layout`, `caveat`? |
| `emulator/player_state` ⚙ | `fields`?, `includeBytes`? | `players[]`, `layout`, `caveat`? |
| `emulator/call_stack` | `maxBytes`?,`maxFrames`? | `pc`,`sp`,`frames[]` |

> ⚙ These decode a game's object records, so **part of each reply is engine-shaped** — and not merely
> per-engine: an object record's tail is an overlay window whose interpretation is chosen by the slot's
> occupant at run time, so it varies **within** one build. The contract therefore fixes the envelope and
> leaves the payload open, deliberately: see §11.25. Four rules follow and all are normative.
> **(1)** Every reply carries `layout` — what the server decoded against, and how — because an unstated
> layout assumption is §4's *confidently wrong information* one level up. A server with no symbol table
> **refuses** with `-32012` rather than decoding from a guessed base.
> **(2)** Engine-specific values travel in the per-item `fields` map, whose keys are the layout's own
> field names and whose values are scalars. A server MUST NOT emit a `fields` key its `layout.engine`
> does not name, and MUST NOT emit decoded bit-name enums for any field. A `fields` **value** follows
> D9 — address-shaped fields as hex strings (category 1), counts and scalars as numbers (category 2) —
> per the layout's own typing of the field: the map's key set is unbounded, its spellings are not.
> **(3)** A `fields` key that is addressable but **not live** for the slot's current occupant MUST be
> omitted or caveated, never reported as a datum — an uninitialised byte returned as a number is a value
> the game never wrote. The same rule one level up: on the two rows that carry `active`, an inactive reply
> carries the slot facts (`slot`, `addr` — and `role` where declared) beside `layout`; the decoded keys are
> omitted, never fabricated.
> **(4)** A slot index past the pool is refused with **`-32602`**, `error.data` carrying the bound, never
> clamped. §11.25 records that the contract is split here: `pixel_attribution` answers `-32004` for a
> structurally identical refusal, and this family follows `scanlines` instead.
> `capabilities.objectDecoders` reports whether this **build** has the handlers — `true` iff at least one
> of these three rows appears in `methods`, per-row servedness remaining `methods` membership (item 23) —
> and never whether a layout was detected; the detect result is on the reply, because `load_symbols` may
> be called after the handshake.
```

*(`emulator/call_stack` is unchanged and shown only for position. Its BLOCKED status is untouched.)*

---

## 3. Delta B — §8 item 20 gains one sentence (M4)

Append inside item 20, after *"…deliberately **NOT** written into the published schema."*:

```markdown
    **The closure is applied at the top level of the result object** — the literal subject of "any result
    key". Objects nested in a result are closed only where their own published subschema closes them
    (`otherMatches.items[]` is the registered case). §2.5 already states the same scope for `params`, in
    item 20's own words; this states it in the item that owns it.
```

**This confirms rather than chooses.** §2.5 at `:666-668` already reads *"The closure is at the top level
of `params` — **item 20's own scope**, for its reason"*, which is contract prose asserting the top-level
reading of item 20. The CR's Q6 said the scope was settled *"only by a code comment in one implementation's
test harness"*; that was wrong in the CR's own favour and §11.25 records the correction. What M4 fixes is
that a load-bearing rule was living in a cross-reference and a test comment instead of in the item that
owns it.

---

## 4. Delta C — the §9 Phase-5 deferral is partially lifted

The entry at `:2277-2278` currently reads:

> - **Config/symbol-driven object decoders** — making `object_slot`/`player_state` not hardcode the
>   aeon vs sonic_hack layouts (Phase 5).

Append to it:

```markdown
  *Partially lifted 2026-08-26 (§11.25):* the **wire shape** of the three ⚙ decoder rows is no longer
  deferred — it is fixed as a closed envelope over an open `fields` payload, with a `layout` descriptor
  making the server's assumption part of the answer. What remains deferred is the *implementation* side: a
  server may still detect its layout however it likes, and the declared field **catalogue** (offsets,
  widths, types) belongs to the planned `debug/struct_layout` op rather than to these rows. Deferring the
  shape was costing the successor a served surface the legacy has, on rows whose result the catalog
  described with a literal ellipsis and a phrase.
```

---

## 5. Delta E — the schema's top-level `description` (M1)

**This is the delta the CR omitted and the ruling made a MUST.** Without it the published artifact carries
`emulator/object_list`'s fragment *and* a sentence naming `object_list` as deliberately unschematized — a
D14-class live contradiction in the very text the CR quotes as its spine.

Find, inside the top-level `"description"` string, the passage beginning `EIGHT §6 ROWS REMAIN
UNSCHEMATIZED` and ending `prints all eight on every run so none goes quiet.` Replace it, in full, with:

```
FIVE §6 ROWS REMAIN UNSCHEMATIZED AND DELIBERATELY SO — z80_registers, read_vdp_registers, read_vsram,
call_stack, log_tail — because each states its result too loosely to transcribe without inventing (a
literal `…` in a key set, an array with no item type, or, for log_tail, a question §10 leaves openly
undecided). The set was EIGHT until 2026-08-26, when object_slot, object_list and player_state left it
under §11.25: they are schematized as a CLOSED ENVELOPE over a typed-open `fields` payload with a REQUIRED
`layout` discriminant, which is the audit's own D-27 unblock condition and the reason the partial-fragment
objection does not reach them — a typed-open map declares its incompleteness as a type, where a partial
fragment asserts a completeness it does not have. That objection stands undiminished for the five above: a
PARTIAL fragment would be worse than none, because item 20's closure would then refuse the conformant
server that emits the keys the fragment omitted, while §2.5 already reads an ABSENT fragment correctly as
'not yet transcribed'. Each remaining row is itemized, with what would unblock it, in
docs/2026-08-22-protocol-schema-audit.md, and the gate at schema/tests/validate_contract_schema.py DERIVES
the blocked set by diffing §6 against `methods` and prints it on every run so none goes quiet — that
output is the authority for the count, exactly as it is for the fragment count, and this prose is not.
```

Three things changed besides the number, and each is deliberate:

1. **`a decode §9 defers to Phase 5` is struck from the reason list.** It was the decoder rows' reason and
   none of the remaining five is a Phase-5 decode. Leaving it would make the parenthetical describe a set
   member that no longer exists.
2. **`prints all eight` becomes `DERIVES … and prints`.** The gate really does derive its list —
   `validate_contract_schema.py:34-36` documents G5 as *"the method rows in protocol.md §6 are diffed
   against the schema's `methods` keys"*, and `:217-223` implements it as
   `have = set(schema["methods"]); missing = [n for n in catalogued if n not in have]`. **So the script
   self-corrects on adoption and only the prose ever lied** — which narrows this delta rather than
   excusing it.
3. **The partial-fragment objection is kept for the five and explicitly *not* retracted.** The three that
   left did not defeat it; they satisfy a different condition. A `$comment` that quietly dropped the
   objection would have made the remaining five look like an accident.

---

## 6. Deltas F–I — the exact schema JSON

Every fragment below was **merged into a working copy of the real schema blob `7b24bced` and validated**;
§8 reports the run.

### 6.1 `$defs` — two additions

```json
"decoderLayout": {
  "type": "object",
  "required": ["engine", "detectedBy", "slotBytes", "slotCount", "baseAddr"],
  "properties": {
    "engine": {
      "type": "string", "minLength": 1,
      "description": "The server's name for the layout it decoded against. A FREE STRING, never an enum: §11.18 makes an emitted enum unwidenable, and this value's whole job is to grow one entry per supported game. Clients compare for equality; they do not switch on a closed set."
    },
    "detectedBy": {
      "type": "string", "minLength": 1,
      "description": "How the layout was chosen. Registered values: 'symbol' (a symbol table resolved the discriminant), 'configured' (an operator named it), 'fallback' (nothing resolved and the server guessed). Free string for `engine`'s reason. A server that answers 'fallback' MUST also emit `caveat`."
    },
    "detectedFrom": {
      "$ref": "#/$defs/symbolName",
      "description": "The symbol whose resolution decided the layout, when detectedBy is 'symbol'. A symbolName, so it round-trips through emulator/lookup_symbol (§4)."
    },
    "slotBytes": {"type": "integer", "minimum": 1, "description": "Stride of one record in bytes. D9 category 2."},
    "slotCount": {
      "type": "integer", "minimum": 0,
      "description": "Total slots the pool spans - what a full scan would cover. NOT the number of active objects, which is the reply's `total`."
    },
    "baseAddr": {
      "$ref": "#/$defs/hex",
      "description": "Bus address of slot 0 (D9 category 1). Lets a client verify every item's addr with one multiplication, which is P1: a decoder reply must be checkable against another instrument on the same bus."
    },
    "pools": {
      "type": "array",
      "description": "The pool vocabulary AS DATA rather than as a schema enum, because an enum would freeze one engine's pool structure into the bus - D-27's stated objection. Ascending firstSlot, contiguous, covering [0, slotCount) where present at all. Absent on an engine with no pool structure. There is deliberately no per-item `pool` key: it is slot vs this table, the derivable key CR-13 struck (the sprites.parsedMax precedent).",
      "items": {
        "type": "object",
        "required": ["name", "firstSlot", "slotCount"],
        "properties": {
          "name": {"type": "string", "minLength": 1},
          "firstSlot": {"type": "integer", "minimum": 0},
          "slotCount": {"type": "integer", "minimum": 0}
        },
        "additionalProperties": false
      }
    }
  },
  "additionalProperties": false,
  "description": "protocol.md §11.25 (CR-D). What the server decoded against, and how - REQUIRED on every reply from a decoder row, never in the handshake, because emulator/load_symbols may be called at any point in a session and the detect branches on whether a symbol resolves, so a handshake-time value is stale by construction. Closed here: it has no allOf for additionalProperties to be blind past (the otherMatches.items[] precedent)."
},
"decodedSlot": {
  "type": "object",
  "description": "protocol.md §11.25 (CR-D). One decoded object record. A SHAPE LIBRARY: types only, deliberately UNCLOSED and carrying NO `required`, because a use-site additionalProperties:false beside allOf is blind past the allOf in BOTH directions and would refuse these very keys (§11.5). Each use site re-lists every permitted key name in its own `properties` (inherited keys as `true`, its additions typed), declares its own `required`, and closes itself. The `object_slot` result is the exception: it is a RESULT top level, where the closure is item 20's harness-side unevaluatedProperties - which does see through allOf - so it declares no additionalProperties at all.",
  "properties": {
    "slot": {"type": "integer", "minimum": 0, "description": "Index into the pool. D9 category 2. Sparse on object_list: empty slots are omitted, so slot numbers skip."},
    "addr": {
      "$ref": "#/$defs/hex",
      "description": "Bus address of the record (D9 category 1). P1: the key that makes the entry checkable with emulator/read and pokeable with emulator/write_memory. The legacy server emits this on object_slot and player_state but NOT on object_list, which is why an object_list reply is unverifiable today."
    },
    "x": {
      "type": "integer",
      "description": "World PIXELS, signed - the integer half of the record's position. Pixels rather than the engine's fixed-point word because pixels are the one value comparable ACROSS layouts; carrying a 16.16 raw would push one engine's fixed-point convention into the bus. A client wanting sub-pixels asks for the field by name or reads the bytes at addr."
    },
    "y": {"type": "integer", "description": "See x."},
    "code": {
      "$ref": "#/$defs/hex",
      "description": "The engine's identity datum for the slot, EXACTLY AS READ, at the offset and width the layout declares. A hex string (D9 category 1) on the sprites.satBase pattern: the representation is category 1 while the computation `ObjCodeBase + code` is what category 2 permits a client to do after parsing. Not a resolved address, because a layout whose identity datum is an object id rather than a code offset has no address to resolve, and a REQUIRED key meaningless on half the layouts is the freezing D-27 forbids. It is also the DISCRIMINANT that says how the record's overlay tail may be read."
    },
    "name": {
      "$ref": "#/$defs/symbolName",
      "description": "The BARE label the server resolved for `code`. §4's identifying spelling: it MUST round-trip through lookup_symbol, so a suffix MUST NOT be stripped for display. Omitted when nothing resolved - never the empty string."
    },
    "nameDisp": {"type": "integer", "minimum": 0, "description": "Bytes past `name`. Present with `name`. §4: 'A displacement is never inside a name string.'"},
    "fields": {
      "type": "object",
      "additionalProperties": {"type": ["number", "string", "boolean"]},
      "description": "TYPED-OPEN MAP. Keys are the LAYOUT's own field names, never names this contract chose; values are scalars. Present iff the request asked for fields. The openness is a declared property of the shape, not a gap the harness fails to reach - the methodSummaries pattern, where the schema pins the value type and prose pins the key provenance. Prose obligations no schema can carry (§11.25): a server MUST NOT emit a key its layout.engine does not name; MUST NOT emit decoded bit-name enums; MUST omit or caveat a key that is addressable but not LIVE for the slot's current occupant; and a value follows D9 - address-shaped fields as hex strings, counts and scalars as numbers, per the layout's own typing of the field."
    },
    "bytes": {
      "$ref": "#/$defs/hex",
      "description": "The whole record verbatim, slotBytes long. Present iff includeBytes. Off by default so the default reply's key set stays fully enumerated by this fragment."
    }
  }
}
```

**`decodedSlot` has no `required` and no `additionalProperties`, and both absences are the point** (M2 and
M3). It is a shape library. Every use site supplies its own `required` and its own closure.

### 6.2 `methods["emulator/object_list"]`

```json
{
  "$comment": "protocol.md §6 (object / player decoders), added 2026-08-26 by §11.25 (CR-D), closing audit D-27 for this row. A CLOSED ENVELOPE over an ENGINE-SHAPED payload: everything the contract owns is enumerated here, and the one part it cannot know - which named fields a given build's record has - is declared open as a typed map rather than left as a gap. §2.4's flat bounded-list spelling, as emulator/sprites uses it, with total/returned/limit/truncated as scalars beside the list. No cursor: the method accepts no continuation param, so a token it issued could never be handed back (§2.4 clause (b), lookup_symbol's ruling). A pure read: not subject to §6's run-control state rule, exactly as for read/sprites/pixel_attribution/scanlines - the envelope's `running` is the whole answer to a torn sample. One semantic divergence from sprites, deliberate: sprites pins total as {const:80} because every slot is an item there, while here an empty slot is NOT an item, so `total` counts ACTIVE objects and the table's size lives in layout.slotCount - two different facts, two homes.",
  "params": {
    "type": "object",
    "unevaluatedProperties": false,
    "properties": {
      "limit": {
        "type": "integer", "minimum": 1,
        "description": "Max entries, from the lowest slot upward. Default: all active slots. Refused, never clamped, below 1 or above limits.maxObjectSlots (-32602)."
      },
      "fields": {
        "type": "array",
        "items": {"type": "string", "minLength": 1},
        "description": "Layout field names to decode into each item's `fields` map. A name the layout does not have is -32602 with error.data.unknownFields carrying the offending names - §2.5's unknownParams shape one level down. The refusal precedes any decode, so a refused request has read nothing."
      },
      "includeBytes": {
        "type": "boolean", "default": false,
        "description": "When true, each item carries `bytes`: its whole record verbatim. Off by default because 66 records at $50 is 5,280 bytes on every call for a caller who wanted two coordinates. Kept rather than deferred to emulator/read because the pool exceeds the catalogued read cap, so 'capture the whole pool' is at minimum two reads with no per-slot slicing and no layout stamp tying the bytes to the decode."
      }
    }
  },
  "result": {
    "allOf": [{"$ref": "#/$defs/replyFields"}],
    "required": ["objects", "total", "returned", "limit", "truncated", "layout"],
    "properties": {
      "objects": {
        "type": "array",
        "description": "ACTIVE slots only, ascending. Presence IS activity: an empty slot is omitted rather than returned with a flag that would always be true. Slot numbers are therefore sparse.",
        "items": {
          "type": "object",
          "allOf": [{"$ref": "#/$defs/decodedSlot"}],
          "required": ["slot", "addr", "x", "y", "code"],
          "properties": {
            "slot": true, "addr": true, "x": true, "y": true, "code": true,
            "name": true, "nameDisp": true, "fields": true, "bytes": true
          },
          "additionalProperties": false
        }
      },
      "total": {
        "type": "integer", "minimum": 0,
        "description": "How many ACTIVE slots exist. NOT the table's size, which is layout.slotCount. total:0 beside truncated:false is 'zero objects' as a stated fact rather than an empty list a client must interpret."
      },
      "returned": {"type": "integer", "minimum": 0, "description": "§2.4 clause (a)."},
      "limit": {
        "type": "integer", "minimum": 1,
        "description": "The ceiling actually applied, echoed so a caller can tell a default from its own request. May differ from the one asked for."
      },
      "truncated": {"type": "boolean", "description": "REQUIRED EVEN WHEN FALSE (§2.4 clause (a))."},
      "layout": {"$ref": "#/$defs/decoderLayout"},
      "caveat": {
        "type": "string", "minLength": 1,
        "description": "§2.4 rule 4: declared because this method CAN emit one, emitted CONDITIONALLY - when layout.detectedBy is 'fallback', or when the symbol table that produced the layout was accepted with binding:'indeterminate' (§4). Never on every reply: a caveat every reply carries is one nobody reads."
      }
    }
  }
}
```

Note the item: **`allOf` for the types, its own `properties` naming all nine legal keys (inherited ones as
`true`), its own `required`, then `additionalProperties: false`.** That is M3's mechanism, and the four
parts are load-bearing together — drop step 2 and the closure refuses the base keys, which §8 demonstrates.

### 6.3 `methods["emulator/player_state"]`

```json
{
  "$comment": "protocol.md §6 (object / player decoders), added 2026-08-26 by §11.25 (CR-D), closing audit D-27 for this row. object_list restricted to the player pool, with roles attached - the player is not a special record but an ordinary slot whose overlay window happens to be the player's. An ARRAY, never per-role keys: the legacy server's top-level key set VARIES BY ROM (player_1/player_2 on one branch, main/sidekick on the other, with an `engine` discriminant present on only one), which this repo's own transcription calls the biggest shape hazard in the eight; an array has a key set that does not vary, `role` carries the label without buying a key, and `layout` carries the discriminant on EVERY reply. No total/returned/truncated and no cursor: the player pool is structurally bounded, and §2.4 clause (d) says a structural bound takes neither. A pure read. NO DECODED BIT NAMES on any field: the legacy's names are invented, its two branches already disagree on the spelling of the same concept (in_air vs air, on_object vs onobject), §11.18 makes an emitted enum unwidenable, and a set-bits list carries strictly LESS than the raw value beside it because it cannot express a clear bit. A client asks for the raw field by name and applies the bit names it already has, from the source that defines them.",
  "params": {
    "type": "object",
    "unevaluatedProperties": false,
    "properties": {
      "fields": {"type": "array", "items": {"type": "string", "minLength": 1}, "description": "As emulator/object_list."},
      "includeBytes": {"type": "boolean", "default": false, "description": "As emulator/object_list."}
    }
  },
  "result": {
    "allOf": [{"$ref": "#/$defs/replyFields"}],
    "required": ["players", "layout"],
    "properties": {
      "players": {
        "type": "array",
        "description": "One entry per player SLOT, ascending - INACTIVE SLOTS INCLUDED, unlike object_list. 'Player 2 is not present' is the answer to the question asked, and a client must not have to infer it from an array's length against a bound it joins from elsewhere.",
        "items": {
          "type": "object",
          "allOf": [
            {"$ref": "#/$defs/decodedSlot"},
            {
              "if": {"required": ["active"], "properties": {"active": {"const": true}}},
              "then": {"required": ["x", "y", "code"]},
              "else": {
                "not": {
                  "anyOf": [
                    {"required": ["x"]}, {"required": ["y"]}, {"required": ["code"]},
                    {"required": ["name"]}, {"required": ["nameDisp"]},
                    {"required": ["fields"]}, {"required": ["bytes"]}
                  ]
                }
              }
            }
          ],
          "required": ["slot", "addr", "active"],
          "properties": {
            "slot": true, "addr": true, "x": true, "y": true, "code": true,
            "name": true, "nameDisp": true, "fields": true, "bytes": true,
            "active": {
              "type": "boolean",
              "description": "Whether this slot holds a live player. REQUIRED, false included: false is the answer, not the absence of one."
            },
            "role": {
              "type": "string", "minLength": 1,
              "description": "The server's label for this slot, from the layout - 'player', 'sidekick', ... A free string, for the reason layout.engine is one (§11.18). May be present on an inactive slot: the label is the slot's, not the occupant's."
            }
          },
          "additionalProperties": false
        }
      },
      "layout": {"$ref": "#/$defs/decoderLayout"},
      "caveat": {"type": "string", "minLength": 1, "description": "As emulator/object_list."}
    }
  }
}
```

**This item is M2.** As the CR first drafted it — `allOf: [decodedSlot]` where `decodedSlot` carried
`required: ["slot","addr","x","y","code"]` — the fragment **refused the reply §7.2 mandates**: an inactive
player is `{slot, addr, active: false}`, which is every reply from a one-player game. The `if`/`then`/`else`
is the fix, and it needed no invention: the schema already carries **eleven** live `if`/`then` sites,
including `scanlines` (`mode` ↔ `rows[].width`), `read` (`space` ↔ `region`), `read_cram` (`line` ↔
palette length), `watchpoint_hits` (`space` ↔ `fc`/`old`), `watchpoint_add` (`mode` ↔ `censusKey`) and the
`emulator/stopped` event (`reason` ↔ `watch`/`breakpoint`).

### 6.4 `methods["emulator/object_slot"]`

```json
{
  "$comment": "protocol.md §6 (object / player decoders), added 2026-08-26 by §11.25 (CR-D), closing audit D-27 for the third row of the family. The single-slot projection of emulator/object_list: the item keys hoisted to the top level, plus `active`, because this row ADDRESSES a slot and emptiness is therefore an answer rather than an omission. When active is false the reply is the slot facts and layout only - the decoded keys are omitted, never fabricated (§11.25, the M2 conditional on both rows that carry active). No consumer asked for it; it travels because leaving one row of a three-row family unschematized would leave the BLOCKED set meaning something its own reason does not say, and because a family with one absence convention is better than a family with two (the legacy emits '' for a missing name here and omits the key there). A slot past the pool is refused -32602 with the bound in error.data, on scanlines' precedent rather than pixel_attribution's -32004 - see §11.25, which records that the contract is split on this. NOTE the closure: this result declares NO additionalProperties, because a result top level composes replyFields and the closure that applies here is item 20's harness-side unevaluatedProperties, which sees through allOf.",
  "params": {
    "type": "object",
    "unevaluatedProperties": false,
    "required": ["slot"],
    "properties": {
      "slot": {
        "type": "integer", "minimum": 0,
        "description": "Index into the pool. Refused with -32602 at or above layout.slotCount, error.data carrying {slot, slotCount}: the fragment cannot bound it because the bound is a property of the loaded game."
      },
      "fields": {"type": "array", "items": {"type": "string", "minLength": 1}, "description": "As emulator/object_list."},
      "includeBytes": {"type": "boolean", "default": false, "description": "As emulator/object_list."}
    }
  },
  "result": {
    "allOf": [
      {"$ref": "#/$defs/replyFields"},
      {"$ref": "#/$defs/decodedSlot"},
      {
        "if": {"required": ["active"], "properties": {"active": {"const": true}}},
        "then": {"required": ["x", "y", "code"]},
        "else": {
          "not": {
            "anyOf": [
              {"required": ["x"]}, {"required": ["y"]}, {"required": ["code"]},
              {"required": ["name"]}, {"required": ["nameDisp"]},
              {"required": ["fields"]}, {"required": ["bytes"]}
            ]
          }
        }
      }
    ],
    "required": ["slot", "addr", "layout", "active"],
    "properties": {
      "layout": {"$ref": "#/$defs/decoderLayout"},
      "active": {"type": "boolean", "description": "Whether the addressed slot holds a live object. REQUIRED, false included: false is the answer, not the absence of one. When false, the decoded keys (x, y, code, name, nameDisp, fields, bytes) are FORBIDDEN - an empty slot's record is bytes the game never wrote, and reporting them would be the uninitialised-byte-as-datum shape rule (3) forbids for `fields`."},
      "caveat": {"type": "string", "minLength": 1, "description": "As emulator/object_list."}
    }
  }
}
```

**This result deliberately declares no `additionalProperties`, and that is not an oversight.** See §9,
flag 2 — a result top level composes `replyFields`, so an `additionalProperties: false` there refuses the
envelope. It is closed by item 20's harness-side `unevaluatedProperties`, which sees through `allOf`.

### 6.5 `capabilities.objectDecoders` — a description, no type change

```json
"objectDecoders": {
  "type": "boolean",
  "description": "Whether THIS BUILD has the object-decoder handlers - never whether a layout was detected. True iff AT LEAST ONE of the §6 decoder rows (object_slot, object_list, player_state) appears in `methods`; per-row servedness is `methods` membership and nothing else (§8 item 23), which this flag never overrides or summarises. Kept a boolean rather than promoted to an object: changing a published key's JSON type is not additive (§11.18), and checkpoints/watchpoints were BORN objects, so they are precedent for new keys and not for retyping an old one. The detect result travels on every decoder reply as `layout`, because emulator/load_symbols may be called at any time and a handshake-time detect is stale by construction. A client that branched on this flag to decide whether a decode would succeed would be reading a build-time constant as a run-time fact. protocol.md §11.25."
}
```

**S4's pin is the "at least one".** Under an "all three" reading, a build that severs D5 or compile-time
drops a row would advertise `false` while serving two decoders — the **under-advertising** hazard §8
item 23 names in terms, whose wire signature is identical to a smaller server.

### 6.6 `limits.maxObjectSlots` — one new OPTIONAL key

```json
"maxObjectSlots": {
  "type": "integer",
  "minimum": 1,
  "description": "Largest `limit` emulator/object_list accepts - THIS SERVER's ceiling, not the catalog's, on the maxProfilerRoutines / maxBreakpoints precedent. A POLICY bound, so it is advertised rather than discovered by refusal, and the bound itself is refused rather than clamped (-32602). OPTIONAL, and its absence is meaningful: a server that applies no ceiling omits it and object_list.limit is then bounded only by layout.slotCount. Optional rather than required because a decoder-less build has no such number and must not be made to invent one. Added 2026-08-26, §11.25."
}
```

`limits` goes from ten declared keys to eleven; the `required` array (`maxRunFrames`, `maxReadLen`,
`maxLineBytes`) **does not change**.

---

## 7. Delta D — §11.25, the amendment entry

Append after §11.24 (currently the last section, ending the file at `:4265`):

```markdown
### 11.25 — 2026-08-26: the decoder rows get a shape — a closed envelope, an open payload, and a server that says what it assumed

**CR-D**, raised by the oracle lane and adjudicated independently (**ADOPT WITH CHANGES**: 62 checkable
claims, 60 held, 2 adjusted, none failed; four MUSTs, seven SHOULDs **and a three-flag delta ruling, all
applied** before this entry was written — the delta amended two of the ruled items on the applier's
objection, `role` surviving inactivity and `object_slot` taking the conditional `required`, and upheld the
third). It closes audit **D-27** for all three rows it names, and it is the first amendment to remove a
row from the schema's BLOCKED set rather than add one to the catalog. **No method is added or removed.**

**The problem, in one sentence.** `emulator/object_list` and `emulator/player_state` are catalogued and
unschematized, so §8 item 20 makes them unimplementable by a conformant successor — the fragment is the
precondition for the handler, not its record — and the schema's own `$comment` says why: *"each states its
result too loosely to transcribe without inventing"*, and *"A PARTIAL fragment would be worse than none"*.
A consumer then filed a demand naming both methods and measuring both `no such method` against the
successor, which §11.23's landing turned from prospective into current: the shim now spawns the successor
by default, so the tools those sessions name are already broken.

**The resolution, and why it is not a loophole.** Item 20's closure is a rule about a **key set**, and the
reason these rows resist transcription is that part of their key set is **not knowable to the contract** —
it is a function of which game is loaded, and, measured in the consumer's own source, of which routine owns
the slot *within one build*: ten declared overlays of one 32-byte window, three of them in release objects,
with the single word at one offset reading as a signed inertia, an object pointer and a pixel extent
depending on the occupant. Those two parts are separable. **The contract closes the part it owns and
declares the other part open as a typed map** — key set unbounded by construction, value shape pinned. That
is not a hole with a nice name: a partial fragment lies about completeness, while a typed-open map states
its incompleteness *as a type*. It is also **the audit's own unblock condition for D-27**, verbatim — *"a
config/symbol-driven decode whose envelope is fixed even though its fields are not — e.g. a declared
`fields` map plus a `layout` discriminant"* — and it has shipping precedent in `methodSummaries`, where the
schema pins the value type and prose pins the key provenance.

| # | What it adds | Why |
|---|---|---|
| **D1** | `layout` — REQUIRED on every decoder reply: `engine`, `detectedBy`, `slotBytes`, `slotCount`, `baseAddr`, `detectedFrom`?, `pools`? | The one REQUIRED key this pre-release window is spent on. A decoder reply that does not say what it decoded against is not degraded information, it is §4's *confidently wrong* information — the `binding: indeterminate` hazard with no `binding` field. On the **reply**, not in `capabilities`, because `load_symbols` may be called at any time and the detect branches on whether a symbol resolves, so a handshake value is stale by construction. `pools` is **data, not an enum**: an enum would freeze one engine's pool structure into the bus, which is D-27's stated objection. |
| **D2** | `emulator/object_list` — flat bounded list of **active** slots; per item a closed core (`slot`, `addr`, `x`, `y`, `code`), optional `name`/`nameDisp`, a typed-open `fields` map, optional `bytes` | §2.4's flat spelling, `sprites`' precedent. One divergence from `sprites`, deliberate: `total` counts **active objects** while the table's size is `layout.slotCount` — two facts, two homes. A per-item `addr` the demand did not ask for, because without it an `object_list` reply is unverifiable against any other instrument. |
| **D3** | `emulator/player_state` — the same item under a `players[]` **array**, plus `active` and `role` | The legacy's top-level key set **varies by ROM** (`player_1`/`player_2` vs `main`/`sidekick`, with `engine` on one branch only) — the biggest shape hazard in the eight. An array's key set does not vary. Inactive slots are **returned with `active: false`**, because "player 2 is absent" is the answer to the question and a client must not infer it from an array's length. |
| **D4** | `capabilities.objectDecoders` keeps its boolean type and gains a pinned meaning | *This build has the handlers*, never *a layout was detected*. `true` iff **at least one** ⚙ row is in `methods`; per-row servedness stays item 23's. Retyping a published key is not additive, and `checkpoints`/`watchpoints` were **born** objects, so they are precedent for new keys and not for retyping old ones. |
| **D5** | `emulator/object_slot` — the single-slot projection, with `active` | No consumer asked. It travels because the audit records its `slot` param was withheld *only* under the no-half-fragment rule, and because a family split two-schematized/one-not would leave the BLOCKED set meaning something its own stated reason does not say. |

**What is deliberately NOT served.** No decoded bit names, on any row. The legacy emits
`{"raw": n, "bits": [...]}` from a hardcoded table whose entries are invented (`b0`, `s2b0`…) and whose two
branches disagree on the spelling of the same concept (`in_air` vs `air`, `on_object` vs `onobject`), so
cross-engine comparison of the field is unsafe *today*, in the only implementation that emits it; §11.18
would then make those strings unwidenable. **And the loss is nil in the direction that matters: a set-bits
list carries strictly less information than the `raw` beside it, because it cannot express a clear bit.** A
client asks for the raw field by name and applies the bit names it already has, from the source that
defines them. This is the one capability this amendment refuses that the legacy ships, and it is refused on
the evidence rather than on taste.

**Five obligations no fragment can express**, listed because a green gate is not conformance: (1) a server
MUST NOT emit a `fields` key its `layout.engine` does not name; (2) no decoded bit-name enums; (3) `layout`
must describe the decode that produced **this** reply, not a cached one; (4) an unknown `fields` name is
refused **before** any decode, so a refused request has read nothing (§2.5's *"the refusal precedes any
effect"*, one level down); (5) a `fields` key that is addressable but **not live** for the slot's current
occupant is omitted or caveated, never reported as a datum.

**No new §8 item, and the reason is not the obvious one.** Five obligations is more than either CR-A or
CR-C carried when each earned a conformance item. The item is declined anyway because those five are
**per-engine and not mechanically checkable by a generic harness**, so an item would add wordage without
verifiability — and an unverifiable conformance item is worse than prose, because it looks like a gate.
Recorded here rather than left unspoken.

**One sentence IS added, to item 20.** Its closure is pinned to the **top level of the result object** —
the literal subject of *"any result key"* — with nested objects closed only where their own published
subschema closes them. This **confirms** rather than chooses: §2.5 already said *"The closure is at the top
level of `params` — item 20's own scope, for its reason"*, and the reference harness has read it that way
since it shipped. What changes is that a load-bearing rule stops living in a cross-reference and a test
comment. Under this pin the `fields` map is legal twice over: it carries its own value subschema *and*
sits below the closure's reach.

**⚑ The contract is split on refusal codes, and this entry records it rather than letting the next drafter
find it.** A slot index past the pool is `-32602`, with the bound in `error.data`. But the contract already
carries two live precedents that disagree on exactly this shape — a coordinate whose legal range is a
runtime fact no static schema can bound. `emulator/pixel_attribution` refuses a dot outside the
**runtime-sized** active display with **`-32004`**, carrying `width`/`height`, and says in terms that no
static schema can express the bound. `emulator/scanlines` (§11.14) refuses an out-of-range row with
**`-32602`**. This family follows `scanlines`, on §2.5's ground that a slot index is a **parameter** and
`-32602` is the params-refusal code with typed `error.data`. The legacy server's `-32004` agrees with
`pixel_attribution` and is not followed. **The divergence is deliberate and now documented; a future
amendment that wants one rule for both should start here.**

**Two hardenings against the only shipping implementation, both taken knowingly.** (1) **No symbols, no
decode**: the legacy falls back to a hardcoded base address at two call sites, which is the confidently-
wrong shape with no `binding` field to reveal it; this family answers **`-32012`** instead, on
`write_memory`'s *"strict by design: relaxing a refusal later is additive (D5); introducing one is not"*.
The escape hatch is genuine — a server with a configured base answers `detectedBy: "configured"` plus a
caveat and loses nothing. (2) **`name` round-trips**: the legacy strips a `_Main` suffix, so a reported
name resolves to nothing; §4 is categorical, and an absent name is an **omitted key**, never `""` (the
legacy uses both conventions on two rows of one family).

**Additivity, in §11.18's form.** 59 fragments before, 62 after; the count is re-derived by parsing the
merged `methods` object per §11.17 clause 7, never carried. **In the never-asking direction nothing
breaks**: no existing fragment is edited except `capabilities.objectDecoders`, which gains a `description`
and keeps its type; `limits` gains one **optional** key and its `required` array is unchanged; no key is
removed from any reply. **In the asking direction, everything these three rows refuse is newly refused**,
because they had no fragment at all — which makes the usual accepted-before/refused-after proof
unavailable and is why the adoption condition below substitutes the CR-BP form.

**What each implementation owes.** The **successor**: implement the rows it advertises, set
`capabilities.objectDecoders` accordingly, and add the names to `methods` — item 23 makes those a single
act. Its symbol machinery already exists. The **legacy server**: no schedule is asked, per §11.21's
*"Legacy is frozen, not migrated"* and §11.23's treatment. Its current replies are non-conformant with
these fragments; that is a frozen state, not a defect list.

**One correction carried in the open, because the amendment rests on the replacement.** The CR's original
load-bearing fact — that the player's ability-scratch fields share bytes — was **wrong**, refuted by a
sentence inside the block quote used to support it, and it was replaced before adjudication by the
ten-overlay measurement this entry uses. The replaced claim's own counterfactual arithmetic was also wrong
(24, not 22) and is corrected in the CR's record. The **design never moved**: closed envelope, typed-open
`fields`, REQUIRED `layout`. One of its two justifications did, and the replacement is layout-level rather
than semantic, measured rather than inferred, and about **objects** rather than only the player — which is
why the row a consumer actually asked for is the one this amendment is most confident about.

*Adoption condition, per §11.6 onward:* registered when **(1)** the gate is GREEN with the fragment count
**re-derived by parsing** and quoted from its output, and its G5 BLOCKED list printing **five** rows —
neither number typed from memory; **(2)** every refusal these fragments introduce is proven red, and the
vectors are proven **wired** by running them against the pre-amendment artifact and observing
`no fragment in the schema` — the CR-BP form, since accepted-before/refused-after cannot exist for a
fragment that did not exist; **(3)** the accepting vectors validate **under item 20's closure**, including
the `object_slot` result, whose top level composes `replyFields` and `decodedSlot` and would be refused by
an `additionalProperties` there; **(4)** the structural conditional is proven in both directions on **both**
rows that carry `active`: for `player_state`, `{slot, addr, active: false}` accepted — `role` beside it also
accepted — and the same item carrying `x` refused; for `object_slot`, `{slot, addr, active: false, layout}`
accepted under item 20's closure and the active reply without `x`/`y`/`code` refused; that set is the whole
of the structural fix M2 and its delta required; and **(5)** the schema's top-level `description` names **five** BLOCKED rows and
no longer names any row this entry schematizes.
```

**If §11.25 is taken before this lands**, renumber and update: the section heading; the `⚙` note in §6
(*"see §11.25"*); §9's *"Partially lifted 2026-08-26 (§11.25)"*; and the four `$comment`/`description`
strings in the schema that cite `§11.25` (`decoderLayout`, `decodedSlot`, the three fragments' `$comment`s,
`objectDecoders`, `maxObjectSlots`). `grep -n '11\.25' contract/` finds all of them.

---

## 8. Delta J — the vectors, and the validation actually run

**22 new cases: 9 accepting, 13 refusing** — 18 as first drafted, plus four the delta ruling
(`docs/2026-08-26-ruling-cr-d-delta.md`) added for M5 and M7. They were not written and hoped for. A
working copy of the real schema blob `7b24bced` was taken, the deltas of §6 merged into it, and the cases
validated with `jsonschema` 4.26.0 (draft 2020-12), result docs merged with the upstream `vectors.json`
envelope exactly as the gate does it. **Every count below is read out of the run, never edited by hand**
(M1(ii)); this is the post-delta run:

```
== G1  merged schema is a valid draft 2020-12 schema
  ok  (whole document + 130 fragments each checked on its own)
== G2  §2.5 / §8 item 22 — request params are closed
  params fragments not closed: none
  fragments 59 -> 62 (parsed, not carried)
== G3/G4  the 22 new vectors
  9 pass-vectors validated, 13 fail-vectors proven red
  7 of the pass-vectors also validated under item 20 closure
== RF  every new vector against the PRE-amendment artifact
  22/22 vectors answer "no fragment in the schema" against the pre-amendment artifact

ALL GREEN
```

**⚑ One number in the pre-delta record was wrong, and the recount caught it.** This document previously
recorded `G4  7 of the pass-vectors also validated under item 20 closure` against 7 pass-vectors. The
closure leg applies only to **server-emitted** payloads — the gate's own `emitted = kind == "result" or
group == "events"` — and two of those 7 passes were `params` cases, so the true pre-delta figure was
**5**, not 7. Re-running the identical harness over this document's own pre-delta text reproduces
`7 pass-vectors validated, 11 fail-vectors proven red` and `5 of the pass-vectors also validated under
item 20 closure`, matching every other line. The post-delta 7 is a real 7: the two new accepting cases
(19 and 22) are both results, so the closure count moves **5 → 7** while pass moves 7 → 9. The coincidence
of the old wrong number and the new right one is exactly why M1(ii) forbids carrying a count.

The `RF` line is the red-first leg in the only form available for a brand-new fragment, and it is the form
CR-BP used: **every one of the 22 fails as `no fragment` against the pre-amendment artifact**, which proves
the cases are wired to the new fragments rather than passing vacuously somewhere else. The `G3` fail count
is the other leg — the gate's own G3 fails a fail-vector the schema accepts, with the message *"this
fragment is vacuous here"*, so thirteen proven-red cases are thirteen places the fragments demonstrably
bite.

| # | method / kind | expect | what it proves |
|---|---|---|---|
| 1 | `object_list` params | pass | all three params |
| 2 | `object_list` params | **fail** | `{pool: "dynamic"}` — `pool` moved to `layout.pools`; an undeclared param is refused (item 22) |
| 3 | `object_list` result | pass | one item carrying **every** optional key, beside the five §2.4 companions and `layout` |
| 4 | `object_list` result | pass | the empty case — `total: 0`, `truncated: false`, and `pools` **absent**, which is legal |
| 5 | `object_list` result | **fail** | an item without `code` — the use-site `required` is real, since `decodedSlot` carries none |
| 6 | `object_list` result | **fail** | `layout` missing — the one REQUIRED spend, proven to bite |
| 7 | `object_list` result | **fail** | `layout` without `detectedBy` — P2's *how*, not just *what* |
| 8 | `object_list` result | **fail** | a `fields` **value** that is an object — the typed-open map is *typed*; this is also the legacy's decoded-bit-name shape being refused by construction |
| 9 | `object_list` result | **fail** | a stranger key (`pool`) on an item — M3's use-site closure really closes |
| 10 | `player_state` result | pass | two players, both active |
| 11 | `player_state` result | pass | **the M2 case** — `{slot, addr, active: false}` beside an active player: the one-player reply §7.2 mandates and the pre-M2 fragment refused |
| 12 | `player_state` result | **fail** | an **active** player with no `x`/`y`/`code` — the `then` branch bites |
| 13 | `player_state` result | **fail** | an **inactive** player carrying `x` — the `else` branch bites |
| 14 | `player_state` result | **fail** | the legacy's `status` object as a sibling key |
| 15 | `object_slot` params | pass | `{slot: 0}` |
| 16 | `object_slot` params | **fail** | `{}` — `slot` is REQUIRED |
| 17 | `object_slot` result | pass | hoisted keys beside `layout` and `active`, validated **closed** — this is the case that proves the envelope survives M3's third form |
| 18 | `object_slot` result | **fail** | `active` missing |
| 19 | `object_slot` result | pass | **the M7 case** — `{slot, addr, active: false, layout}`, the empty-slot reply the pre-delta fragment refused for missing `x`/`y`/`code`; validated under item 20's closure |
| 20 | `object_slot` result | **fail** | the same doc with `active: true` — the `then` bites, so M7 loosened the empty case without loosening the occupied one |
| 21 | `object_slot` result | **fail** | `active: false` carrying `x: 0` — the `else` bites; the uninitialised byte as a datum, one level up from rule (3) |
| 22 | `player_state` result | pass | **the M5 case** — an active player beside `{slot, addr, active: false, role: "sidekick"}`; the pre-delta fragment forbade `role` here |

Cases **19–22 are the delta ruling's**, added by M5 and M7. Two auxiliary measurements the delta names but
deliberately does not spend a vector on, reproduced against the same merged copy: `object_slot` with
`active: false` carrying `bytes` is **refused** (the `else`'s seventh name, and the reason `bytes` stays in
it — an empty slot's bytes are exactly the unwritten record); and `object_slot` carrying `role` is
**refused under item 20's closure** as an unevaluated key, `role` being declared on the player item only.

**The exact cases**, to be appended to `vectors.json`'s `cases` array — the file's own
`{method, kind, expect, doc, why}` shape. Result docs are merged with the file's `envelope` by the
runner, so none of them carries `frame`/`mclk`/`running`/`droppedEvents`:

```json
[
  {
    "method": "emulator/object_list",
    "kind": "params",
    "expect": "pass",
    "doc": {
      "limit": 8,
      "fields": [
        "anim",
        "mapping_frame"
      ],
      "includeBytes": false
    },
    "why": "§11.25 - all three params, §6's row."
  },
  {
    "method": "emulator/object_list",
    "kind": "params",
    "expect": "fail",
    "doc": {
      "pool": "dynamic"
    },
    "why": "§11.25 - `pool` moved to layout.pools; an undeclared param is -32602 (§8 item 22)."
  },
  {
    "method": "emulator/object_list",
    "kind": "result",
    "expect": "pass",
    "doc": {
      "objects": [
        {
          "slot": 2,
          "addr": "0x00FF8E50",
          "x": 1024,
          "y": 320,
          "code": "0x2A18",
          "name": "Obj_Ring_Main",
          "nameDisp": 0,
          "fields": {
            "anim": 3,
            "mappings": "0x00A1C4",
            "onScreen": true
          },
          "bytes": "0x2A18000000000400"
        }
      ],
      "total": 1,
      "returned": 1,
      "limit": 40,
      "truncated": false,
      "layout": {
        "engine": "aeon-sst",
        "detectedBy": "symbol",
        "detectedFrom": "Player_1",
        "slotBytes": 80,
        "slotCount": 66,
        "baseAddr": "0x00FF8DB0",
        "pools": [
          {
            "name": "player",
            "firstSlot": 0,
            "slotCount": 2
          },
          {
            "name": "dynamic",
            "firstSlot": 2,
            "slotCount": 40
          }
        ]
      }
    },
    "why": "§11.25 - one item carrying every optional key, beside the five §2.4 companions and layout."
  },
  {
    "method": "emulator/object_list",
    "kind": "result",
    "expect": "pass",
    "doc": {
      "objects": [],
      "total": 0,
      "returned": 0,
      "limit": 40,
      "truncated": false,
      "layout": {
        "engine": "aeon-sst",
        "detectedBy": "symbol",
        "slotBytes": 80,
        "slotCount": 66,
        "baseAddr": "0x00FF8DB0"
      }
    },
    "why": "§11.25 - 'zero objects' as a stated fact: total 0 beside truncated false, not an empty list a client must interpret. `pools` absent, which is legal."
  },
  {
    "method": "emulator/object_list",
    "kind": "result",
    "expect": "fail",
    "doc": {
      "objects": [
        {
          "slot": 2,
          "addr": "0x00FF8E50",
          "x": 1024,
          "y": 320
        }
      ],
      "total": 1,
      "returned": 1,
      "limit": 40,
      "truncated": false,
      "layout": {
        "engine": "aeon-sst",
        "detectedBy": "symbol",
        "slotBytes": 80,
        "slotCount": 66,
        "baseAddr": "0x00FF8DB0"
      }
    },
    "why": "§11.25 - an item without `code`. The five core keys are required at the use site, since decodedSlot carries no required of its own."
  },
  {
    "method": "emulator/object_list",
    "kind": "result",
    "expect": "fail",
    "doc": {
      "objects": [],
      "total": 0,
      "returned": 0,
      "limit": 40,
      "truncated": false
    },
    "why": "§11.25 - `layout` missing. The one REQUIRED key this amendment spends the pre-release window on, proven to bite."
  },
  {
    "method": "emulator/object_list",
    "kind": "result",
    "expect": "fail",
    "doc": {
      "objects": [],
      "total": 0,
      "returned": 0,
      "limit": 40,
      "truncated": false,
      "layout": {
        "engine": "aeon-sst",
        "slotBytes": 80,
        "slotCount": 66,
        "baseAddr": "0x00FF8DB0"
      }
    },
    "why": "§11.25 - layout without `detectedBy`. P2: HOW the layout was chosen is part of the answer, so decoderLayout's required set is not vacuous."
  },
  {
    "method": "emulator/object_list",
    "kind": "result",
    "expect": "fail",
    "doc": {
      "objects": [
        {
          "slot": 2,
          "addr": "0x00FF8E50",
          "x": 1024,
          "y": 320,
          "code": "0x2A18",
          "fields": {
            "status": {
              "raw": 6,
              "bits": [
                "in_air"
              ]
            }
          }
        }
      ],
      "total": 1,
      "returned": 1,
      "limit": 40,
      "truncated": false,
      "layout": {
        "engine": "aeon-sst",
        "detectedBy": "symbol",
        "slotBytes": 80,
        "slotCount": 66,
        "baseAddr": "0x00FF8DB0"
      }
    },
    "why": "§11.25 - a `fields` VALUE that is an object. The typed-open map is TYPED: its keys are unbounded, its value shape is not. This is the vector that proves the openness is bounded, and it is the legacy's decoded-bit-name shape being refused by construction."
  },
  {
    "method": "emulator/object_list",
    "kind": "result",
    "expect": "fail",
    "doc": {
      "objects": [
        {
          "slot": 2,
          "addr": "0x00FF8E50",
          "x": 1024,
          "y": 320,
          "code": "0x2A18",
          "pool": "dynamic"
        }
      ],
      "total": 1,
      "returned": 1,
      "limit": 40,
      "truncated": false,
      "layout": {
        "engine": "aeon-sst",
        "detectedBy": "symbol",
        "slotBytes": 80,
        "slotCount": 66,
        "baseAddr": "0x00FF8DB0"
      }
    },
    "why": "§11.25 - a stranger key on an item. The use-site closure really closes, which is what the M3 factoring had to preserve."
  },
  {
    "method": "emulator/player_state",
    "kind": "result",
    "expect": "pass",
    "doc": {
      "players": [
        {
          "slot": 0,
          "addr": "0x00FF8DB0",
          "x": 96,
          "y": 656,
          "code": "0x0100",
          "name": "Player_Main",
          "nameDisp": 0,
          "active": true,
          "role": "player"
        },
        {
          "slot": 1,
          "addr": "0x00FF8E00",
          "x": 64,
          "y": 656,
          "code": "0x0100",
          "active": true,
          "role": "sidekick"
        }
      ],
      "layout": {
        "engine": "aeon-sst",
        "detectedBy": "symbol",
        "detectedFrom": "Player_1",
        "slotBytes": 80,
        "slotCount": 66,
        "baseAddr": "0x00FF8DB0",
        "pools": [
          {
            "name": "player",
            "firstSlot": 0,
            "slotCount": 2
          }
        ]
      }
    },
    "why": "§11.25 - two players, both active. No total/returned/truncated: a structural bound takes neither (§2.4 clause (d))."
  },
  {
    "method": "emulator/player_state",
    "kind": "result",
    "expect": "pass",
    "doc": {
      "players": [
        {
          "slot": 0,
          "addr": "0x00FF8DB0",
          "x": 96,
          "y": 656,
          "code": "0x0100",
          "active": true,
          "role": "player"
        },
        {
          "slot": 1,
          "addr": "0x00FF8E00",
          "active": false
        }
      ],
      "layout": {
        "engine": "aeon-sst",
        "detectedBy": "symbol",
        "slotBytes": 80,
        "slotCount": 66,
        "baseAddr": "0x00FF8DB0"
      }
    },
    "why": "§11.25 M2 - THE ONE-PLAYER REPLY. §7.2 mandates {slot, addr, active:false} with the rest omitted, and the pre-M2 draft's fragment refused it. This vector is the fix's whole point and the commonest reply the method will ever send."
  },
  {
    "method": "emulator/player_state",
    "kind": "result",
    "expect": "fail",
    "doc": {
      "players": [
        {
          "slot": 0,
          "addr": "0x00FF8DB0",
          "active": true
        }
      ],
      "layout": {
        "engine": "aeon-sst",
        "detectedBy": "symbol",
        "slotBytes": 80,
        "slotCount": 66,
        "baseAddr": "0x00FF8DB0"
      }
    },
    "why": "§11.25 M2 - an ACTIVE player with no x/y/code. The `then` branch bites: the conditional loosens the inactive case without loosening the active one."
  },
  {
    "method": "emulator/player_state",
    "kind": "result",
    "expect": "fail",
    "doc": {
      "players": [
        {
          "slot": 1,
          "addr": "0x00FF8E00",
          "active": false,
          "x": 0
        }
      ],
      "layout": {
        "engine": "aeon-sst",
        "detectedBy": "symbol",
        "slotBytes": 80,
        "slotCount": 66,
        "baseAddr": "0x00FF8DB0"
      }
    },
    "why": "§11.25 M2 - an INACTIVE player carrying x. The `else` branch bites: 'the rest are omitted' is a schema rule, not prose a server may not read. An x of 0 for an absent player is §4's confidently-wrong shape in miniature."
  },
  {
    "method": "emulator/player_state",
    "kind": "result",
    "expect": "fail",
    "doc": {
      "players": [
        {
          "slot": 0,
          "addr": "0x00FF8DB0",
          "x": 96,
          "y": 656,
          "code": "0x0100",
          "active": true,
          "status": {
            "raw": 6,
            "bits": [
              "in_air"
            ]
          }
        }
      ],
      "layout": {
        "engine": "aeon-sst",
        "detectedBy": "symbol",
        "slotBytes": 80,
        "slotCount": 66,
        "baseAddr": "0x00FF8DB0"
      }
    },
    "why": "§11.25 - the legacy's `status` object as a sibling key. Refused: decoded bit names are declined on this row, and the item's key set is closed."
  },
  {
    "method": "emulator/object_slot",
    "kind": "params",
    "expect": "pass",
    "doc": {
      "slot": 0
    },
    "why": "§11.25 - the one required param."
  },
  {
    "method": "emulator/object_slot",
    "kind": "params",
    "expect": "fail",
    "doc": {},
    "why": "§11.25 - `slot` is REQUIRED. This row addresses a slot; a call that names none is not a smaller request, it is object_list."
  },
  {
    "method": "emulator/object_slot",
    "kind": "result",
    "expect": "pass",
    "doc": {
      "slot": 7,
      "addr": "0x00FF9010",
      "x": 512,
      "y": 288,
      "code": "0x2A18",
      "name": "Obj_Spring_Main",
      "nameDisp": 4,
      "active": true,
      "layout": {
        "engine": "aeon-sst",
        "detectedBy": "symbol",
        "slotBytes": 80,
        "slotCount": 66,
        "baseAddr": "0x00FF8DB0"
      }
    },
    "why": "§11.25 - the item keys HOISTED to the result top level beside layout and active. Validated closed, which is what proves the envelope survives a result that $refs decodedSlot: additionalProperties would have refused the stamp, unevaluatedProperties sees through the allOf."
  },
  {
    "method": "emulator/object_slot",
    "kind": "result",
    "expect": "fail",
    "doc": {
      "slot": 7,
      "addr": "0x00FF9010",
      "x": 512,
      "y": 288,
      "code": "0x2A18",
      "layout": {
        "engine": "aeon-sst",
        "detectedBy": "symbol",
        "slotBytes": 80,
        "slotCount": 66,
        "baseAddr": "0x00FF8DB0"
      }
    },
    "why": "§11.25 - `active` missing. On a row that ADDRESSES a slot, emptiness is an answer and the flag is required, false included."
  },
  {
    "method": "emulator/object_slot",
    "kind": "result",
    "expect": "pass",
    "doc": {
      "slot": 1,
      "addr": "0x00FF8E00",
      "active": false,
      "layout": {
        "engine": "aeon-sst",
        "detectedBy": "symbol",
        "slotBytes": 80,
        "slotCount": 66,
        "baseAddr": "0x00FF8DB0"
      }
    },
    "why": "§11.25 M7 - THE EMPTY-SLOT REPLY. A row that addresses a slot must be able to answer 'nothing here': the slot facts and layout, with the decoded keys omitted rather than read out of a record the game never wrote. The pre-delta fragment required x/y/code unconditionally and refused this."
  },
  {
    "method": "emulator/object_slot",
    "kind": "result",
    "expect": "fail",
    "doc": {
      "slot": 1,
      "addr": "0x00FF8E00",
      "active": true,
      "layout": {
        "engine": "aeon-sst",
        "detectedBy": "symbol",
        "slotBytes": 80,
        "slotCount": 66,
        "baseAddr": "0x00FF8DB0"
      }
    },
    "why": "§11.25 M7 - an ACTIVE object_slot with no x/y/code. The `then` branch bites: the conditional loosens the empty case without loosening the occupied one."
  },
  {
    "method": "emulator/object_slot",
    "kind": "result",
    "expect": "fail",
    "doc": {
      "slot": 1,
      "addr": "0x00FF8E00",
      "active": false,
      "x": 0,
      "layout": {
        "engine": "aeon-sst",
        "detectedBy": "symbol",
        "slotBytes": 80,
        "slotCount": 66,
        "baseAddr": "0x00FF8DB0"
      }
    },
    "why": "§11.25 M7 - an INACTIVE object_slot carrying x. The `else` branch bites. An x of 0 for an empty slot is the uninitialised byte reported as a datum, one level up from the rule (3) that forbids it for `fields`."
  },
  {
    "method": "emulator/player_state",
    "kind": "result",
    "expect": "pass",
    "doc": {
      "players": [
        {
          "slot": 0,
          "addr": "0x00FF8DB0",
          "x": 96,
          "y": 656,
          "code": "0x0100",
          "active": true,
          "role": "player"
        },
        {
          "slot": 1,
          "addr": "0x00FF8E00",
          "active": false,
          "role": "sidekick"
        }
      ],
      "layout": {
        "engine": "aeon-sst",
        "detectedBy": "symbol",
        "slotBytes": 80,
        "slotCount": 66,
        "baseAddr": "0x00FF8DB0"
      }
    },
    "why": "§11.25 M5 - `role` SURVIVES INACTIVITY. The label is the slot's, not the occupant's, and decoderLayout.pools is closed at {name, firstSlot, slotCount}, so per-slot roles are on the wire nowhere else: forbidding `role` here would have deleted the answer, not displaced it to a join. The pre-delta fragment refused this."
  }
]
```

---

## 9. Flagged: applied as ruled, objection recorded — three items, each demonstrated, all three now ruled

**None of these was changed unilaterally.** A post-adjudication change rides a delta back to the same
adjudicator; the point of this section is to make that delta cheap to write. **It was written, and it
ruled** — `docs/2026-08-26-ruling-cr-d-delta.md`, same adjudicator, same standard, 2026-08-26. Each flag's
outcome is recorded at the head of its subsection below; the flag texts themselves are left standing as
the record of what was raised and on what grounds.

| Flag | Outcome | Instrument | Artifact change |
|---|---|---|---|
| 1 — `role` forbidden on an inactive player | **AMENDED** | **M5** — `role` survives inactivity | One name out of the player `else`, one sentence on `role`'s description, §7.2's mandate sentence, §10.5.3(A)'s player row, vector 22 |
| 2 — M3's third use site | **UPHELD**, and M3's own wording amended to match the applied form | **M6** | **None.** §10.5.2's precision paragraph and `decodedSlot`'s exception sentence are ratified as the authoritative statement of M3 |
| 3 — `object_slot` refuses the empty slot | **AMENDED** | **M7** — the M2 conditional, on the sibling row M2 misnamed | `object_slot.result` takes the `if`/`then`/`else` with `role` absent, `required` becomes `[slot, addr, layout, active]`, `active`'s description and the `$comment` grow, rule (3) gains a sentence, §10.5.3(A)'s row, vectors 19–21 |

**The hold this document carried is released.** The delta held the amendment on M7 and set the releasing
condition itself — M5 and M7 applied, §8's validation re-run over the amended fragments with the four new
vectors, and its output quoted here — adding that release comes *"by application, not by a further delta"*
and that only a non-green run returns. §8's run is **ALL GREEN**. Nothing goes back.

### Flag 1 — `role` is forbidden on an inactive player

> **⚑ RULED: AMENDED (M5).** `role` survives inactivity. The delta upheld this objection on ground
> *stronger* than the flag claimed: the flag offered "a client joins `layout.pools`" as the cost of being
> wrong, and the delta measured that `decoderLayout.pools` items are **closed** at
> `{name, firstSlot, slotCount}` — pool names, not per-slot roles — so **the join does not exist** and
> forbidding `role` deleted the answer rather than displacing it. The flag's own recorded counter-cost ("a
> key present sometimes and absent sometimes for no stated reason") did not survive inspection either:
> `role` is already OPTIONAL on active items, and the stated reason is the one the fragment carries — the
> label is the slot's, not the occupant's. Applied above; proven by vector 22.

`player_state`'s `else` branch forbids `role`, per the ruling's *"require exactly `slot`, `addr`, `active`
and forbid the rest"* and per §7.2's own *"the rest are omitted"*. **The objection:** `role` is arguably a
fact about the **slot** — like `slot` and `addr`, which do survive — so a client wanting to say *"the
sidekick slot is empty"* must otherwise join `layout.pools` to learn which slot the sidekick is. If that is
right, the fix is one name removed from the `else`'s `anyOf` and one word added to §7.2. **Cost of being
wrong in the current direction: a client joins a table.** Cost of being wrong the other way: a key that is
present sometimes and absent sometimes for no stated reason. The conservative choice was taken.

### Flag 2 — M3's third use site takes a different form, and this was decided rather than asked

> **⚑ RULED: UPHELD (M6) — the applied form stands and M3's wording was amended to match it. Zero artifact
> change.** The delta found the departure *forced, not chosen*: M3 was written for `decodedSlot`'s use
> sites without noticing that the third is a result top level, which composes `replyFields` — so M3's own
> four steps applied there are the §11.5 trap from a fourth direction, and re-listing the envelope's keys
> locally to appease the keyword would fossilize `replyFields`' key set into one fragment. It also
> measured the supporting claim rather than accepting it: **zero** of the 59 committed fragments carry
> `additionalProperties` on a result top level (parsed, not grepped). §10.5.2's precision paragraph and
> `decodedSlot`'s exception sentence are ratified as the authoritative statement of M3.

M3 says *"each of the three use sites re-lists every permitted key name … and closes with
`additionalProperties: false`"*. Two of the three are nested item objects and do exactly that. The third,
`object_slot`'s **result top level**, cannot — and the reason is the very trap M3 exists to close, arriving
from a fourth direction. Demonstrated against the merged schema:

```
=== M3 third use site: additionalProperties:false on a RESULT top level ===
  with additionalProperties:false -> 1 errors, first: Additional properties are not allowed
      ('addr', 'code', 'droppedEvents', 'frame', 'mclk', 'running', 'slot', 'x', 'y' were unexpected)
  as ruled (no additionalProperties) -> accepted
```

It refuses the **envelope** — `frame`, `mclk`, `running`, `droppedEvents` — as well as the hoisted keys. So
that use site takes the universal result form: `allOf: [replyFields, decodedSlot]`, its own `properties`
for its additions, its own `required`, and **no `additionalProperties` at all**, closed by item 20's
harness-side `unevaluatedProperties`, which *does* see through `allOf`. No published result top level in
the schema carries `additionalProperties` today and this one must not become the first. **Judged a
precision consistent with M3's purpose, not a departure** — but decided here, so it is flagged.

For completeness, the same run confirms M3's premise, which is why the mechanism had to change at all:

```
=== M3: the broken middle form, demonstrated ===
  a perfectly conformant item: {"slot":0,"addr":"0x00FF8DB0","x":96,"y":656,"code":"0x0100","active":true}
  REFUSED by the broken middle form: Additional properties are not allowed
      ('addr', 'code', 'slot', 'x', 'y' were unexpected)
=== M3: the ruled form accepts the same item ===
  errors: none
```

### Flag 3 — `object_slot` requires all five core keys unconditionally, and M2's bug survives there

> **⚑ RULED: AMENDED (M7) — and this was the blocker.** The delta agreed the flag is right for the reason
> it gives: *this is M2's own defect surviving in the sibling row M2 named by hand.* M2 diagnosed the
> disease precisely — a row carrying `active` REQUIRED "false included" must not unconditionally require
> the decoded keys — and then wrote `object_slot` into the unconditional column without noticing §8.2
> gives that row `active` too. **The `required` set the original ruling settled by name is unsettled and
> replaced**: `["slot", "addr", "layout", "active"]`, with the same `if`/`then`/`else` the player item
> carries, `role` absent. `bytes` deliberately stays forbidden on an inactive reply — an empty slot's
> `bytes` are exactly the unwritten record, so `includeBytes: true` against an empty slot returns
> `active: false` and no `bytes`; a future CR wanting raw bytes of empty slots has `emulator/read`.
> M7 is only expressible *because* flag 2 was upheld: the conditional lands as a third `allOf` member
> beside the two `$ref`s, which the universal result form accommodates and a closed form would have
> fought. Applied above; proven by vectors 19, 20 and 21.

The ruling's M2 is explicit about the use sites: *"`object_list` items and `object_slot` require all five;
the player item requires them conditionally."* That is applied as written. **The objection is that M2's own
reasoning reaches `object_slot` too.** `object_slot` carries `active`, so `active: false` is a reachable
reply — and under the ruled `required` set that reply is refused:

```
=== FLAG 3: an INACTIVE object_slot under the ruled fragment ===
  REFUSED: 'x' is a required property
  REFUSED: 'y' is a required property
  REFUSED: 'code' is a required property
```

`{slot, addr, active: false, layout}` is the honest answer for an empty slot, and the alternative — emitting
`x`, `y` and `code` read out of a record the game has not written — is precisely the *uninitialised byte
reported as a datum* that §7.3 and rule (3) of the ⚙ note forbid one level down, for `fields`. The fix
would be the same `if`/`then`/`else` the player item now carries, with `role` absent. **It is not applied**,
because the ruling settled the `object_slot` `required` set by name and this parcel does not overrule it at
the point of application. **This is the one flag that would change published wire behaviour**, and it should
be settled before §11.25 lands rather than after.

---

## 10. What the oracle lane owes after this lands

Nothing in this document is server work, and this parcel ships none. For the record, so the serve parcel
can be scoped:

1. Implement `emulator/object_list`, `emulator/player_state` and — if D5 survives — `emulator/object_slot`,
   against these fragments rather than against the legacy's replies (the consumer's stated reference policy:
   *do NOT A/B against the legacy bridge's shape*).
2. Set `capabilities.objectDecoders` to `true` and add the names to `methods` — item 23 makes those one act,
   and `engine.rs:1393` currently emits `false`.
3. Emit `limits.maxObjectSlots` if the build applies a ceiling; omit it otherwise.
4. Refuse rather than fall back: `-32012` with no symbol table, `-32602` for an unknown `fields` name (with
   `error.data.unknownFields`) and for a slot past the pool (with the bound), never clamping.
5. Vendored-schema refresh: the vendored copy at `crates/oracle-aether/.../bus-protocol.schema.json` hashes
   to the upstream blob today and must be re-vendored in the same change.

**The legacy C++ server is asked for nothing**, per §11.21's *"Legacy is frozen, not migrated"*.

---

## 11. ⟨RUNTIME⟩ — for the controller's foreground follow-up

Unchanged: **four**, carried from the CR's §12.7 and endorsed verbatim by the ruling, which added none and
recorded that nothing in it depends on any of them. **Applying the ruling created no new one.** Background
agents must not touch the emulator MCP; none was attempted here.

1. What the legacy server actually replies for both methods on a live aeon ROM — every shape in the CR and
   the ruling is a source read.
2. Whether `Object_RAM`, `ObjCodeBase`, `Dynamic_Slots`, `System_Slots`, `Effect_Slots` and
   `Object_RAM_End` all resolve in a current `s4.debug.lst` — D1's `pools` needs them, and `pools`'
   optionality is the fallback if any is absent.
3. `Player_1`'s address in a current build vs the demand's `$FF8DB0` vs the committed fixture's
   neighbourhood — the discrepancy is already proven documentary; confirming it live makes it measured.
4. Whether consumer agent sessions call these tools today — no source sweep can count invocations.

---

## 12. BLOCKED

**Nothing.** Every M and every S was applied; every anchor resolved; the amendment is drafted in full.

**Not reached, and stated rather than implied:** the empyrean gate was **read, not run** — running it
requires writing the deltas into `empyrean/contract/`, which is the landing lane's commit and not this
one's. What was run instead is the equivalent validation described in §8, against a working copy of the
committed blob in a scratch directory: same schema bytes, same envelope, same closure keyword, same
draft. The gate's own G5 and G6 stages were not reproduced (G5 needs `protocol.md` amended in place; G6
validates the spec's example payloads, which this amendment does not touch). No `cargo` was run. No server
was started.

**And the three flags in §9 are no longer open.** They went back to the same adjudicator as a delta, which
ruled M5 amend / M6 uphold / M7 amend; all three are applied above, §8's validation was re-run over the
amended fragments with the delta's four new vectors, and it is **ALL GREEN**. The delta's own releasing
condition is therefore met and the hold on this handoff is **released**: this document is ready to hand
over. One number was corrected in the re-run and is recorded at §8 (the pre-delta `G4` closure count was
7 where the run says 5) — a recount finding, not a defect in any fragment.
