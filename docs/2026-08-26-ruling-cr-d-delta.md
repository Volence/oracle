# Delta ruling on CR-D — the three §15.3 flags (2026-08-26, same adjudicator standard)

**This is a delta to `docs/2026-08-26-ruling-cr-d.md`, not a re-adjudication.** The original ruling
(ADOPT WITH CHANGES, M1–M4 + S1–S7) and this delta were both issued by **Claude Fable 5**, under the same
standard: every checkable claim measured firsthand before it is relied on, sibling repos read only at
committed revisions via `git show`, no emulator, no `cargo`, no server, nothing committed to any `main`,
`docs/lane-status.json` untouched. Scope is confined to the three flags recorded at §15.3 of
`docs/2026-08-26-cr-d-object-decoders.md` and what they strictly require; **no settled item is reopened.**

Artifacts adjudicated, all at oracle `main` `613ca3e86a11f55096784aa825294d0dd5a9fd5c`: the applied CR
(blob `7ea9e82f2332d9978898188f1580e65f618d53c8`), the drafted amendment handoff
`docs/2026-08-26-cr-d-amendment-handoff.md` (blob `b7e96e98c43bf350fa8e1a704f9dd03319cdd95f`), and the
original ruling (blob `57eb9e5bd6af282054e77bf012d8556b3e5afb5c`). The contract was read at empyrean
`78d432235090ae53848f4f6725f36ac148ff1ef4` — the same revision the ruling and the handoff used — and the
schema blob extracted from it by `git show` hashes to `7b24bcedc24f0a6aa7dd4504f4e2f9bf63e4cda7`, matching
both documents' citation.

## Verdict, in one table

| Flag | Verdict | Change | Blocks the amendment? |
|---|---|---|---|
| 1 — `role` forbidden on an inactive player | **AMEND** | **M5** — `role` survives inactivity | Rides the standing hold |
| 2 — M3's third use site | **UPHOLD the applied form**; the ruled wording is amended to match | **M6** — M3 restated; zero artifact change | No |
| 3 — `object_slot` refuses the empty slot | **AMEND** | **M7** — the M2 conditional, applied to the sibling row M2 misnamed | **YES — the hold stands until M7 (and M5) are applied** |

**The hold is released by application, not by a further delta.** M5 and M7 are given below as exact,
measured text; applying them is mechanical. The releasing condition is: both applied, the handoff's §8
validation re-run over the amended fragments with the four new vectors, and its output quoted in the
handoff — the same M1(ii)/(iii) discipline the original ruling imposed. If that run is not ALL GREEN,
the discrepancy comes back as a new delta; nothing else does.

---

## Every flag's claim, measured before ruling

All four measurements were reproduced here from scratch — a working copy of the committed schema blob
`7b24bced` with the handoff's §6 fragments merged in, validated with `jsonschema` 4.26.0 (draft 2020-12),
result docs carrying the full envelope. None was taken from the flag text.

1. **Flag 3's refusal reproduces exactly.** `{slot, addr, active: false, layout}` + envelope against the
   shipped `object_slot` result: refused, three errors — `'x' is a required property`, `'y'`, `'code'`.
   The active control is accepted. This is unconditional-`required` semantics; there is no reading of the
   shipped fragment under which the empty-slot reply passes.
2. **Flag 2's refusal reproduces exactly.** The same active reply with `additionalProperties: false`
   added at the result top level: refused — `'addr', 'code', 'droppedEvents', 'frame', 'mclk', 'running',
   'slot', 'x', 'y' were unexpected`. The envelope is among the refused keys, and doubly so: the keyword
   is blind past the local `allOf` *and* past `replyFields`' own internal `allOf` → `stamp`, where
   `frame`/`mclk`/`running` actually live.
3. **The handoff's "no published result top level carries `additionalProperties`" is true.** Parsed, not
   grepped: across all 59 fragments in the committed blob, zero `result` objects carry the keyword.
4. **Flag 1's ground is stronger than the flag states.** The flag says a client denied `role` on an
   inactive item "must otherwise join `layout.pools` to find out which slot the sidekick is." Measured:
   `decoderLayout.pools` items are **closed** at `{name, firstSlot, slotCount}` — pool names, not per-slot
   roles. The join the flag offers as the fallback **does not exist**. Under the shipped `else`, "the
   sidekick slot is empty" is not expressible from any reply of this method, or of any other.

---

## M5 — Flag 1: `role` survives inactivity (AMEND)

**Ruling.** The original M2 text — *"require exactly `slot`, `addr`, `active` and forbid the rest"* — was
a faithful enforcement of §7.2's *"the rest are omitted"*, and both were wrong about one name. `role` is,
by the fragment's **own** description, *"The server's label for this **slot**, from the layout"* — a fact
about the slot, exactly as `slot` and `addr` are, and those survive inactivity for exactly that stated
reason. Measurement 4 removes the only counter-weight: since per-slot roles are on the wire nowhere else,
forbidding `role` here does not push the client to a join, it deletes the answer. And the original
ruling's own Q1 leaned on this key in this posture — *"`active: false` as a stated fact and `role` are
not in any `object_list` reply"* was part of why D3 is not derivable — so forbidding `role` on the
inactive entry undercuts the ruling's own adoption argument for the row. The handoff's recorded cost of
amending ("a key present sometimes and absent sometimes for no stated reason") does not survive
inspection: `role` is OPTIONAL on active items already, and the stated reason is the one the fragment
carries — the label is the slot's, not the occupant's.

**Exact changes:**

1. **Schema, `methods["emulator/player_state"]` player item, the `else` branch:** delete
   `{"required": ["role"]}` from the `not.anyOf` list. The list becomes the seven names
   `x`, `y`, `code`, `name`, `nameDisp`, `fields`, `bytes`.
2. **Same item, `role`'s `description`:** append the sentence
   *"May be present on an inactive slot: the label is the slot's, not the occupant's."*
3. **CR §7.2, the mandate sentence** becomes: *"When `active` is `false`, `slot` and `addr` are still
   present (they are facts about the slot, not the object), `role` may still be present (it, too, is a
   fact about the slot — the layout's label for it), and the rest are omitted."* The ⚑ block's `else`
   list drops `role`; the *"one consequence worth naming"* paragraph is marked resolved by this delta.
4. **CR §10.5.3(A) table, player row, conditional column:** the `active: false` list drops `role`.

**Measured:** with the fix, `{slot: 1, addr, active: false, role: "sidekick"}` is accepted (also under
item 20's closure); the bare `{slot, addr, active: false}` stays accepted; `{…, active: false, x: 0}`
stays refused. The shipped fragment refuses the `role` case — the control that proves the edit is the
one that changes behaviour.

## M6 — Flag 2: the applied form is UPHELD; M3's wording is amended to match

**Ruling.** The implementer's reading is correct and its departure was forced, not chosen. M3's *"each of
the three use sites … closes with `additionalProperties: false`"* was written for `decodedSlot`'s use
sites without noticing that the third is a **result top level**, which composes `replyFields` — so M3's
own four steps, applied there, are the §11.5 trap arriving from a fourth direction (measurement 2: the
envelope itself is refused, and re-listing the envelope's keys locally to appease the keyword would
fossilize `replyFields`' key set into one fragment). The universal result form the implementer shipped —
`allOf: [replyFields, decodedSlot]`, own `properties` for additions, own `required`, **no
`additionalProperties`**, closed at test time by item 20's `unevaluatedProperties`, which does see
through `allOf` — is what every other result top level in the published schema does (measurement 3:
zero exceptions), and it is item 20's design as pinned at Q6/M4. **No artifact changes under M6.**

**The amended M3, replacing the original ruling's operative sentence:** *"`decodedSlot` is factored
unclosed (types only, no `required`). At each use site that is a **nested item object** — `object_list`'s
items and `player_state`'s items — the site re-lists every permitted key name in its own `properties`
(inherited keys as `true` schemas, its additions typed), declares its own `required`, and closes with
`additionalProperties: false`. At a use site that is a **result top level** — `object_slot` — the site
takes the universal result form: `allOf` refs for the shapes, its own `properties` and `required`, and no
`additionalProperties` at all, the closure being item 20's harness-side `unevaluatedProperties`. No
`unevaluatedProperties` appears in the published artifact."* The handoff's §10.5.2 precision paragraph
and the `decodedSlot` description's exception sentence are hereby ratified as the authoritative statement
of M3; nothing in either needs editing.

## M7 — Flag 3: `object_slot` takes the M2 conditional (AMEND; this is the blocker)

**Ruling.** The flag is right, and it is right for the reason it gives: **this is M2's own defect
surviving in the sibling row M2 named by hand.** The original M2 diagnosed the disease precisely — a
row that carries `active` REQUIRED *"false included"* must not unconditionally require the decoded keys —
and then, in its parenthetical fix, wrote `object_slot` into the unconditional column without noticing
that §8.2 gives that row `active` too (*"this row addresses a slot, so emptiness is an answer"*).
Measurement 1 shows the consequence: the honest empty-slot reply is refused unless the server emits `x`,
`y` and `code` for a record the game never wrote — the *uninitialised byte reported as a datum* that the
same CR's §7.3 and the ⚙ note's rule (3) forbid one level down for `fields`, and §4's confidently-wrong
shape at the level above it. The `required` set the original ruling settled by name is hereby unsettled
and replaced.

**Exact changes:**

1. **Schema, `methods["emulator/object_slot"].result`:** `required` becomes
   `["slot", "addr", "layout", "active"]`, and the `allOf` gains a third member — the same conditional
   the player item carries, `role` absent:

```json
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
```

   (`layout` and `caveat` are unchanged; only `active`'s description grows. The fragment's `$comment`
   gains one sentence after *"…emptiness is therefore an answer rather than an omission."*:
   *"When active is false the reply is the slot facts and layout only - the decoded keys are omitted,
   never fabricated (§11.25, the M2 conditional on both rows that carry active)."*)

2. **Delta A, the ⚙ note:** append one sentence to rule (3), so the conditional exists in prose as well
   as in schema on both rows: *"The same rule one level up: on the two rows that carry `active`, an
   inactive reply carries the slot facts (`slot`, `addr` — and `role` where declared) beside `layout`;
   the decoded keys are omitted, never fabricated."*
3. **CR §10.5.3(A) table, `object_slot` row** becomes: unconditional `slot`, `addr`, `layout`, `active`;
   conditional `active: true` → also `x`, `y`, `code`; `active: false` → none of `x`, `y`, `code`,
   `name`, `nameDisp`, `fields`, `bytes`.

**Measured, with the fix in place:** `{slot, addr, active: false, layout}` + envelope is **accepted**,
including under item 20's `unevaluatedProperties` closure; the full active reply stays accepted under the
same closure; `{slot, addr, active: true, layout}` without `x`/`y`/`code` is **refused** (the `then`
bites); `active: false` carrying `x` — and, separately, carrying `bytes` — is **refused** (the `else`
bites). The merged schema with M5 and M7 applied remains a valid draft 2020-12 schema
(`check_schema` clean).

**Why `bytes` stays in the `else`, stated rather than assumed:** an empty slot's `bytes` are exactly the
unwritten record; `includeBytes: true` against an empty slot returns `active: false` and no `bytes`, for
rule (3)'s reason and for symmetry with the player item. A future CR that wants raw bytes of empty slots
has `emulator/read`.

---

## Interaction of the three — checked, and it is benign in both directions

Flags 2 and 3 both live at `object_slot`'s result top level, and **M7 is only expressible because flag 2
was decided the way it was**: the conditional lands as a third `allOf` member beside the two `$ref`s,
which the universal result form accommodates and a closed-with-`additionalProperties` form would have
fought (the `if`/`else` subschemas evaluate no properties, so the harness-side `unevaluatedProperties`
is undisturbed — measured, both replies green under closure). Had flag 2 been reversed, M7 would have
needed a different mechanism; upholding flag 2 is therefore load-bearing for M7's exact text.

M5 and M7 also interact, pleasantly: with `role` freed from the player `else` and absent from
`object_slot`'s, **the two `else` lists become identical** — the seven decoded keys — so the family has
one inactive convention, not two. That is the same coherence argument (one family, one convention) that
carried D5 in the original ruling.

## Consequential changes — named, bounded, nothing else reopened

1. **Vectors (handoff Delta J):** four new cases, numbered 19–22 in the table's own ordering, in
   `vectors.json`'s `{method, kind, expect, doc, why}` shape (result docs merged with the envelope by the
   runner, as the file does it):
   - **19** `object_slot` result **pass** — `{"slot": 1, "addr": "0x00FF8E00", "active": false,
     "layout": {…}}` — the empty-slot reply this delta exists for; validated under item 20's closure
     (the G4 leg).
   - **20** `object_slot` result **fail** — the same doc with `"active": true` — the `then` bites.
   - **21** `object_slot` result **fail** — `active: false` carrying `"x": 0` — the `else` bites.
   - **22** `player_state` result **pass** — an active player beside
     `{"slot": 1, "addr": "0x00FF8E00", "active": false, "role": "sidekick"}` — M5 proven.
   The counts move 18 → 22, 7 → 9 accepting, 11 → 13 refusing — **and per M1(ii) those numbers are
   re-derived from the validation run's output when applied, never carried from this document.** The
   delta table's row J and §8's header change accordingly.
2. **§11.25 adoption condition (4)** (handoff Delta D) is replaced with: *"**(4)** the structural
   conditional is proven in both directions on **both** rows that carry `active`: for `player_state`,
   `{slot, addr, active: false}` accepted — `role` beside it also accepted — and the same item carrying
   `x` refused; for `object_slot`, `{slot, addr, active: false, layout}` accepted under item 20's closure
   and the active reply without `x`/`y`/`code` refused; that set is the whole of the structural fix M2
   and its delta required;"*. Conditions (1)–(3) and (5) are untouched.
3. **Handoff §9** records each flag's outcome (flag 1 AMENDED/M5, flag 2 UPHELD/M6, flag 3 AMENDED/M7)
   with a pointer here; **CR §15.3** likewise. The §11.25 entry's headline claim *"four MUSTs and seven
   SHOULDs, all applied"* becomes *"…and a three-flag delta ruling, all applied"* or equivalent — it must
   not ship claiming the ruling was applied without naming that part of it was amended on delta.

Nothing else in either artifact is touched. In particular: Delta A's row texts, Delta B, Delta C,
Delta E, `object_list`'s fragment, `decoderLayout`, `decodedSlot`, `objectDecoders`, `maxObjectSlots`,
and vectors 1–18 are all unchanged (vector 17 remains valid — an active reply with the hoisted keys).

## Not reached

The empyrean gate itself was again read, not run — running it requires writing into
`empyrean/contract/`, which is the landing lane's act and out of this ruling's scope; the equivalent
scratch-blob validation above is the same substitution the handoff's §8 made, on the same schema bytes
and the same validator version. No runtime item was attempted; the original ruling's four ⟨RUNTIME⟩
items stand unchanged, and this delta adds none.

## Provenance

| | |
|---|---|
| Delta ruled by | **Claude Fable 5** — the same model, under the same standard, as the original ruling this delta amends |
| Written in | oracle worktree `/home/volence/sonic_hacks/oracle/.claude/worktrees/agent-ac1cb84094f542a88`, branch `ruling-cr-d-delta`, cut from `main` `613ca3e86a11f55096784aa825294d0dd5a9fd5c` |
| Artifacts read at | oracle `613ca3e8…` (CR blob `7ea9e82f…`, handoff blob `b7e96e98…`, original ruling blob `57eb9e5b…`); empyrean `78d43223…` via `git show`, schema blob `7b24bced…` verified by `git hash-object` |
| Measurements | `jsonschema` 4.26.0, draft 2020-12, against a scratch merge of the committed blob + the handoff's §6 fragments; flags 2 and 3 reproduced before ruling; M5 and M7 fixture-tested in both directions, with and without item 20's closure; the no-`additionalProperties`-on-result-top-level claim verified by parse over all 59 fragments |
| Runtime | none — no `cargo`, no emulator, no `mcp__oracle__*`, no server; `docs/lane-status.json` untouched; nothing committed to any `main` |
