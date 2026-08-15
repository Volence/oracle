# Ruling — CR-13 (undocumented result keys) and CR-14 (`otherMatches`), 2026-08-15

An un-framed adjudication pass, run because both change requests block the same downstream work: writing
the 12 missing per-method schema fragments. Ruled **together**, which the ruling confirms was the right
framing — `checkpoint_list`'s surplus `total`/`returned`/`limit` is a CR-13 row *and* is the house
bounded-array envelope whose normative status CR-14 exists to settle. Ruling CR-13's row first would have
fixed a spelling before the rule that governs it existed.

## Verdicts

**CR-13 — split the block.** Register eight classes with conditions, **remove two outright**, restructure
two. The CR expected "register, not remove", and that expectation was right for most of the list and
**wrong for exactly the entries its own triage flagged least**.

**CR-14 — the envelope wins, but not as drafted.** `otherMatches` becomes a bounded, truncation-flagging
object with object items and **no continuation fields at all**. The CR's three-way ranking is correct; its
option 1 would have made normative the very dead token its own last paragraph flags.

---

## What the ruling verified rather than accepted

- **`stoppedAtFrame`/`stoppedAtMclk` are the stamp's numbers — checked three ways**, and independently
  reproduced by this session before the ruling arrived. Code (`StopRecord` is captured at the halt and the
  stamp is computed on the same engine thread immediately after `dispatch` returns, machine paused, nothing
  able to advance between), plus both live branches: reached (`0`/`0`) and deadline (`3`/`2688140`),
  identical each time. **Duplicates by construction, not coincidence.**
- **CR-13's eleven table rows are accurate** — every key set reproduced byte-for-byte on a live server, and
  every quoted §6/§4/§8/D14 sentence checked verbatim. One nit: the heading says "ten methods"; the table
  covers twelve.
- **★ The structural ask names the wrong keyword, and it was proven.** `additionalProperties: false` on the
  result fragments **rejects every conformant reply** — the stamp and `droppedEvents` arrive through
  `allOf: [$ref replyFields]`, which `additionalProperties` cannot see in draft 2020-12. Reproduced here:

  ```
  as published            : True
  + additionalProperties  : False   ('droppedEvents','frame','mclk','running' were unexpected)
  + unevaluatedProperties : True    ...and catches ('stoppedAtFrame','stoppedAtMclk')
  ```

  The working keyword is **`unevaluatedProperties: false`**. This corrects text written in CR-13 itself.

## Four surplus key sets the sweep missed

The probe warned its count was a floor. It was, by at least four:

| method | beyond its row |
|---|---|
| `emulator/registers` | `usp`, `ssp` — and its schema fragment enumerates **no** register properties at all |
| `emulator/read_memory` | `symbolDisp` when symbols resolve |
| `emulator/lookup_symbol` | `query`, `exact`, `rawName`, `rawAddr`, `ambiguous`, `synthetic`, `demangled`, `caveat` — **today's largest unruled surplus** |
| `emulator/load_symbols` | `binding`, `moduleCount` (the CR's parenthetical) |

Hence **condition 7**: rerun the key-set sweep across *every* advertised method's success branch before
writing the fragments, so the amendment enumerates the complete surplus once rather than per-discovery.

## Two things CR-13's triage missed entirely

- **`release_all.released` is a hardcoded constant.** `Ok(json!({"released": true}))` — it can never be
  false and carries zero bits. Verified. The CR's own defence list conspicuously skips it. **Remove**, or
  respec it to carry the buttons actually released, parallel to `hold.held`. Default: remove. Not
  amputation — there is nothing attached.
- **`otherMatches` emits two different item shapes**: `{name, addr}` on the exact-demangled branch and
  `{name, demangled, addr}` on the prefix branch. Verified. Any amendment must pin one.

## The dispositions

**REGISTER** (§6/§2.1 + schema, same pass): `initialize.limits` (as a top-level key like `timingBasis` —
the hosted 120-frame *refusal* is undiscoverable otherwise); `status.romBytes/romPath/symbolsPath` (paths
are already pervasive on this bus — `reload_rom`, `load_symbols`, `screenshot`, `romReloaded` all carry
one — so the trust-model objection proves too much; add one D8 sentence); `read_memory.region` + the missed
`symbolDisp`; `press.port` / `hold.port,held` (the two-controller surface the catalog never grew);
`pause`/`resume.wasRunning` (D12's `reached` logic transposed); `registers.usp,ssp`;
`checkpoint_list.total/returned/limit` **under the new convention**; `load_symbols.binding,moduleCount`.

**REMOVE** (server): `run_to.stoppedAtFrame`/`stoppedAtMclk` — §6.1 has already ruled this exact case for
`restore` (*"its result **is** the machine stamp — no extra fields are needed and none should be
invented"*), and keeping them teaches clients the stamp is not the answer, which is the one lesson D11
exists to prevent. And `release_all.released`.

**RESTRUCTURE:**
- **`caveat` — register once, generically.** It appears on at least six methods and only `run_to`'s row
  admits it. New **§2.4 "Shared result conventions"**: optional, human-readable, clients SHOULD surface and
  MUST NOT parse. This ends the class permanently instead of one key at a time. Advisory: a caveat that is
  *always* present (`read_memory`'s constant debug-read string) is documentation wearing signal's clothes,
  and clients learn to ignore it — prefer conditional caveats.
- **`methodSummaries` — register with a MUST-derive clause, or drop it; not register bare.** Bare, it is a
  second op inventory and D4 retired those by name. But the drift D4 fears is structurally absent when both
  lists come from one registry — the device D16 already uses for `timingBasis`. Clause: MUST derive from
  the same registry as `methods`, key set MUST equal `methods`, values non-normative, clients MUST NOT use
  it for capability discovery. With it, a documentation surface the planned MCP wrapper genuinely needs;
  without it, remove.

## On the "adopt at the shape that shipped" precedent

Sound **conditionally, and this ruling is the condition**. The precedent was never "shipped shapes get
registered" — each adoption carried a merits pass, and two changed the draft on the way in. The hazard is
the expectation CR-13 itself voices ("expect the answer to be register") becoming self-fulfilling, at which
point §8's prohibition is a filing ritual and the implementation leads de facto. Two removals and two
restructurings out of one block are the teeth.

## CR-14 in detail

**Adopted shape** — no `cursor`, no `nextCursor`, no required `limit`:

```
"otherMatches": { "items": [ {name, addr, demangled?}, … ], "total": n, "returned": k, "truncated": bool }
```

**How far it generalises — and it has a real home.** The prior adjudication was right that "§2's
bounded-array rule" did not exist, but **§11.1 already reserved the lot**: *"if a later amendment gives
list-shaped results a home of their own, the sentence should move there whole."* This is that amendment.
§2.4 states: (a) a list bounded by *policy* MUST carry `total` and `truncated`; (b) continuation exists only
on methods that accept it, and **a method that accepts no cursor MUST NOT emit one**; (c) §6.1's cursor
invariant moves there whole; (d) §11.3's dichotomy refined — *policy bound → flag it, and cursor it only
where continuation is supported; structural bound → neither*. `otherMatches` is on the policy side (the
spec chose 5; the symbol table holds thousands), so §11.3's `candidates` exception does not cover it.

Schema home: **`$defs/boundedList`** for a list that is a *field* of a result. `checkpoint_list`, a
list-as-whole-result, **keeps its catalogued flat spelling** and its three surplus keys are registered
there. *The one point the ruling says it would not overturn an owner who disagreed:* unifying
`checkpoint_list` onto the nested container while there are still no clients is defensible under §8 item
16's own "cheapest moment" argument.

**The dead token, both halves.** §8 item 16 stands: any `cursor` on the wire is a string, so
`rpc::bounded_array`'s numeric tokens are a server bug regardless of outcome. And **no continuation without
a continuation param** — a token that can never be handed back trains clients that handles are ignorable
and publishes the server's internal position for nothing, which is what D9 category 4's opacity exists to
prevent. If symbol *browsing* is ever wanted it is a new cursored method, raised as its own CR;
`lookup_symbol` is a resolution primitive, and a client that hits `truncated: true` refines its prefix.

## ★ The structural ask, reconciled with D5

Adopt the goal, correct the mechanism, **relocate the enforcement**. Closure must **not** go in the
published schema: D5 makes fields additive by design, so a client validating replies against a vendored
schema would break on the *next conformant amendment* — closure would weaponise stale schemas against
conformant servers, which is D5's preserved-defect argument inverted.

The reconciliation: **closure binds servers, additivity protects clients**, so the closure belongs where
only servers stand — the conformance harness. A new §8 item: *a server's suite MUST fail on result keys
absent from the method's schema fragment; an unknown key is a change request, never a shipment* —
implemented as `unevaluatedProperties: false` applied at test time, which is what the validator's own
`AETHER_STRICT` sketch was already reaching for.

---

## Conditions of adoption, in applying order

**One `empyrean` pass** (D14: prose and schema together):

1. New **§2.4 "Shared result conventions"** — `caveat`; the bounded-list rule (a)–(d); §6.1's cursor
   invariant moved whole with a pointer left behind.
2. **§4 rewritten** for `lookup_symbol`: `otherMatches` as the bounded object with one pinned item shape —
   and while there, rule on the eight-key success surface rather than inheriting it.
3. **§6 / §2.1 rows** per the REGISTER list; `run_to` and `release_all` rows **unchanged**; one D8 sentence
   on paths.
4. **Schema**: `$defs/boundedList`; amended `lookup_symbol`; `registers` finally gains its properties; then
   the 12 missing fragments written **from the amended §6 rows, never from the wire**.
5. **New §8 item** for the harness-side closure.

**`oracle-next`, after the amendment lands:**

6. Remove `stoppedAtFrame`/`stoppedAtMclk` and `released`; fix or bypass `bounded_array`'s numeric tokens;
   reshape `lookup_symbol`'s envelope and unify its item shape; add the `methodSummaries` derivation
   conformance; delete the CR-14 registry entry (the liveness test will force it); wire the strict mode.
7. **Before step 4 is written**: rerun the key-set sweep across every advertised method's success branch.

## Where it disagreed with the register

**CR-13**: the block framing; `stoppedAt*` (its "harder look" was warranted — they go); `released` (missed,
vacuous); the structural ask's keyword *and* its artifact. **CR-14**: option 1's full-envelope drafting
(the dead token would have become normative). Nothing else — CR-14's facts, ranking and rejection
reasoning all survived checking.
