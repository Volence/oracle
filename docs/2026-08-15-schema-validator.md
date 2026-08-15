# A schema validator in the test loop (2026-08-15)

Contract **§8 item 15**: *"[the schema] is the authority on wire shapes; a server's own tests SHOULD
assert real messages against it rather than against a reading of this prose."* **D14** makes
`schema/bus-protocol.schema.json` normative for wire shapes and `protocol.md` normative for behaviour.

This is the build-out of that item, and the report on what it found. It follows
`docs/2026-08-15-wire-conformance-probe.md`, which measured what a validator would catch **before** one
existed. The headline is that the probe was right about the shape of the answer and **wrong about its
size**: its F1 reported one live schema violation; there are three, and F1's own author reached the second
of them independently while this was in flight (CR-14 / probe F5). Two instruments landing on the same
defect from opposite directions is the useful part; only CR-15 is new to this pass.

---

## What was built

**A validator on the single funnel.** `crates/oracle-aether/tests/common/mod.rs` has one `Client`, and
its `recv()` is the only place a line the server sent enters a test. Every line is now validated there,
so no test in this crate can receive an off-contract shape without failing. `Client` learns the method
each outstanding request asked for — from `call()`, and from `send_raw()` for the many tests that write
the request line by hand — so a reply can be checked against the right per-method schema.

| line | schema |
|---|---|
| every line, no exceptions | `anyMessage` |
| a success reply | `methods.<name>.result`, keyed off the request it answers |
| the handshake reply | `handshake.initialize.result` |
| an error reply | `$defs/errorObject`, reached *through* `anyMessage` — it needs no separate arm |
| a notification | `events.<name>.params`, keyed off its own method name |

**A real validator, not a hand-rolled one.** `jsonschema 0.49`, `default-features = false`, a
**dev-dependency**: this crate's runtime deps stay `oracle-core` + `serde_json`. Hand-rolling a checker
for a normative artifact would produce a second *reading* of it, which is the failure item 15 exists to
prevent. The schema document is not itself a JSON Schema — `anyMessage`, `handshake`, `events`, `methods`
are plain object keys — so each fragment is lifted out and compiled with the root `$defs` spliced in so
its `#/$defs/...` refs resolve.

**A vendored schema, proved fresh.** `crates/oracle-aether/tests/contract/bus-protocol.schema.json` is
compiled in with `include_str!`, so the suite is hermetic and gives the same verdict on a machine with no
`empyrean/` checkout. `PROVENANCE.md` beside it records the source commit
(`18a551e`; schema last touched by `627e5e4`) and SHA-256. A separate test byte-compares it against the
upstream file, so a contract edit turns this suite red and forces an explicit re-vendor commit — which is
the auditable record of "we adopted contract revision X".

**Not validated: what the client sends.** Decided, not missed. Several tests deliberately send malformed
params to assert `-32602`; making outgoing validation work needs a per-call opt-out threaded through
every such site, and the server is the conformance subject. Recorded in the module doc so the next reader
knows.

---

## Coverage, as the harness itself prints it

```
--- Aether wire-schema coverage (contract §8 item 15) ---
advertised methods: 20   result schema present: 8   absent: 12
  COVERED   (8): emulator/checkpoint, emulator/checkpoint_drop, emulator/checkpoint_list,
                 emulator/lookup_symbol, emulator/read_memory, emulator/registers,
                 emulator/restore, emulator/run_to
  UNCOVERED (12): emulator/hold, emulator/load_symbols, emulator/pause, emulator/press,
                  emulator/read_vram, emulator/release_all, emulator/reload_rom, emulator/resume,
                  emulator/run_frames, emulator/screenshot, emulator/state_hash, emulator/status
  schematized but not advertised (1): emulator/write_cram
  events with a params schema (3): emulator/resumed, emulator/romReloaded, emulator/stopped
  => envelope coverage 100% of lines; result coverage 8/20 methods.
```

Reproduces probe finding F3 exactly. Not a defect — the schema's own title says **SEED** and §6 says the
remaining fragments are *"completed mechanically during emulator conformance"* — but a harness that
validates every reply and does not say this reads as though it checks everything.

The uncovered list is **pinned**. A newly advertised method cannot quietly join the unchecked pile, and
completing a fragment forces the list to shrink in the same commit that re-vendors the schema. Writing
the 12 missing fragments is explicitly out of scope: probe finding F4 measured ~10 methods emitting result
keys that appear in no contract text, and writing schemas from what this server emits would encode the
implementation as the contract — the inversion §8 forbids.

---

## What it caught — the part that is not a null result

The probe's F1 said *"the only schema-level failure on the live wire is the checkpoint `id`"*. **That is
understated.** Wiring the validator into the loop turned **four existing tests** red on **two** further
divergences.

One of the two — `lookup_symbol.otherMatches` — was found **independently and concurrently** by the main
session, which re-probed with a symbol listing loaded and registered it as **CR-14** / probe finding
**F5** while this work was in flight. Two instruments reaching the same defect from opposite directions
(a hand-driven probe that widened its sample; a validator that inherited the suite's) is a corroboration
worth more than either alone, and it is recorded here as such rather than claimed twice. The other is new.

### CR-15 — `id: null` on a parse error

Three tests failed: `invalid_json_is_32700_with_a_null_id`, `batches_are_refused_with_32600`,
`an_over_long_line_is_refused_without_desyncing_the_connection`.

JSON-RPC 2.0 §5 **mandates** `"id": null` when the id could not be detected. The schema's `$defs/id` is
`["integer","string"]`. §2 is titled *"The envelope (JSON-RPC 2.0)"* and §8 item 2 says to adopt it — so
the adopted standard requires the value the schema's type union excludes. That is a spec bug under D14,
not a server bug: the alternatives are inventing an id (telling a client the failure belongs to a call it
never made) or omitting it (breaking JSON-RPC 2.0 a second way).

### CR-14 — `lookup_symbol.otherMatches`

One test failed: `arrays_are_bounded_cursored_and_flag_truncation`. The schema types the key as an array
of strings; this server emits `rpc::bounded_array`'s `{items, total, returned, cursor, limit, truncated,
nextCursor}`, because *"every array is bounded, cursored, and flags truncation"* is a standing
non-negotiable here — asserted, as it happens, by the very test that went red. A **type** conflict, not
an additive one, and a real design question: `checkpoint_list` already carries the same envelope spelled
out longhand, so the contract arguably wants a `$defs/boundedArray`.

### Why the probe missed both, and what that argues

Neither is exotic. The probe drove six error paths and thirteen methods and saw neither, for one reason
each:

- every one of its error paths sent a **parseable** request, so it never produced a null id;
- `otherMatches` is emitted only on the *partial-match* path — a prefix hit, or an ambiguous demangled
  name. The probe's symbol path is listed in its own method table as `-32013 symbol not found`, the
  no-match-at-all branch, which returns an error and no result at all. The key never reached the wire.

A one-shot probe validates the messages the ROM and the script it chose happen to produce. A validator in
the test loop validates the messages the **whole suite** produces — the 69 integration tests that
actually drive a server (checkpoints 20, methods 22, handshake 13, events 8, hosted 6), every error path
anyone bothered to write a test for, and the *interesting* branches of the methods rather than the first
one a script reaches. That is the argument for item 15 saying *tests*
rather than *a check*, and it is worth more than the two findings themselves.

### How both are handled — `KNOWN_CONTRACT_DIVERGENCES`

Neither is resolved here, and neither is silenced. D14 calls a disagreement of this kind a **spec bug
awaiting amendment**, and in both cases the *server's* shape is arguably the better one, so the ruling is
the owner's — changing the server unilaterally would decide the question by implementing it, which is the
inversion §8 forbids.

They are therefore **registered**, in one named list in `common::schema`, each entry carrying its CR
number, method, JSON path, one-line summary, and a **canonical instance of the diverging shape**. The
registry has four properties, and the last matters most:

1. **The suite stays green**, so a known ruling-pending divergence does not read as a regression.
2. **Each allowance is narrow and separately fenced.** CR-15's is exactly `error` + `id: null` + a code in
   `{-32700, -32600}`, and it substitutes a placeholder id rather than skipping the envelope — a parse
   error that lost its `data`, or whose `data` lost `droppedEvents`, still fails. CR-14's lifts the one key
   out and checks it against the **house** bounded-array shape instead — a bounded array missing
   `truncated` still fails, and `addr` is still held to `$defs/hex`. This is the difference between
   registering a divergence and exempting a method from validation.
3. **An entry that stops firing fails the suite.** `every_registered_divergence_is_still_live` asserts each
   canonical message is *still* rejected by `check_incoming_strict` (the no-allowances verdict) and *is*
   accepted by `check_incoming`. When a CR is ruled on and the schema re-vendored, that goes red and the
   entry must be deleted in the same commit. Liveness is keyed to the **schema** rather than to observed
   traffic on purpose: a counter of live firings could only see the messages its own test binary produced,
   which is precisely the sampling weakness that let CR-14 through a 33-message probe.
   `every_registered_divergence_names_a_real_change_request` additionally greps the ledger for each CR
   heading, so an entry cannot point at nothing.
4. **The list prints beside the coverage report**, and the report ends by saying in words that this server
   is *not* fully schema-conformant:

```
  KNOWN CONTRACT DIVERGENCES (2) — registered, ruling-pending, NOT conformant:
    CR-14 emulator/lookup_symbol $.otherMatches — schema says an array of strings; we emit the house
          bounded-array object with {name,demangled,addr} items — wrong container AND wrong element type
    CR-15 <envelope> $.id — $defs/id is [integer,string]; JSON-RPC 2.0 §5 MANDATES null when the id
          could not be detected (-32700 parse error, -32600 invalid request)
  => this server is NOT fully schema-conformant. A green suite means "no unregistered divergences",
     which is a weaker and more useful claim.
```

That last property is the point rather than a nicety. CR-13 and CR-14 exist because undocumented wire
shapes were **invisible**; an allowance list that hid them would rebuild the problem inside the instrument
built to find it. And the validator's panic message says so directly — a new failure is told, in the
panic, that a genuinely-pending divergence belongs in the registry with a CR number, never silenced at the
call site.

Both are raised in `docs/2026-08-14-aether-change-requests.md`. The contract repo was **not** edited.

---

## What it provably cannot catch

### The known blind spot: §8 item 13 (probe F2), now an executable fact

`emulator/stopped` carrying `reason: "step"` for a completed `run_frames` **passes** — `step` is a legal
enum member. The rule that picks between two legal members is §3 prose, and D14 puts behaviour under the
prose. Of the two mechanical conformance items in this arc the validator catches one (item 16, the
checkpoint id) and is blind to the other **by construction**.

This is now `tests/schema_conformance.rs::the_schema_cannot_express_section_8_item_13_and_this_test_proves_it`,
which asserts the mislabel passes — so if the schema ever grows a way to express item 13, that test goes
red and tells us. The rule itself stays where it has to: behavioural assertions in `tests/events.rs`,
both of which now carry a note saying, in effect, *do not delete this because the schema checks events
now — it does not check this*.

### A second blind spot, found while building the controls

`anyMessage` is a `oneOf` over **both directions** of the wire. `$defs/notification` does forbid an `id`,
but a line with `jsonrpc` + `id` + `method` + `params` is a perfectly legal `$defs/request` — so an event
that grew an `id` is accepted, indistinguishable from a client request travelling the wrong way. It would
also skip `events.<name>.params` in this harness, which keys off "has no `id`". Closing it needs
`anyMessage` split into client→server and server→client halves: a contract change, out of this slice.
Recorded as `anymessage_is_bidirectional_so_a_stray_id_on_an_event_slips_through`, which asserts the hole
is still there so it cannot close silently. The server does not do this today, and `tests/events.rs`
asserts a notification carries no id, directly.

### The general shape of it

A schema checks *which keys exist, of what type, in what range*. It cannot check that a value is the
**right** one among several legal ones, that two fields agree, that a sequence is ordered, or that a
number means what it says. Every one of those lives in `protocol.md` and needs a behavioural test. Item
15 is worth having; it is not a substitute for §8 items 10–14 and 17–19.

---

## Anti-vacuity

A validator that accepts everything is a green suite over an unchecked wire — the same failure as the
volatility test that was a name grep and the assertion that passed with zero enqueues. So the harness
proves it **rejects**, and each control names the field it caught:

| control | proves |
|---|---|
| a conformant reply and a conformant event pass | it does not reject unconditionally — without this every row below is satisfied by a broken validator |
| `droppedEvents` removed | §2.3 / D17 / item 18: present even at zero |
| a **numeric** checkpoint `id` | item 16 / D9 cat. 4 — a real regression guard: this shape was live in this tree until today. Also proves the **keying** works: the envelope alone accepts it, so it can only be caught by having reached the right per-method schema |
| a `stopped` `reason` outside the enum | §3's enum is closed |
| an envelope with both `result` and `error` | §2's success/failure discriminant |
| an `error` with no `data`, and one whose `data` lost its stamp | §2.2 / §2.3: `data` is always present |
| a hex field that lost its `0x` | `$defs/hex` is a pattern, not just "a string" |
| the two allowances, four fences each | each allowance is as narrow as claimed |
| every registered divergence's canonical message | it is *still* rejected without allowances — the registry cannot rot after a ruling |
| every registered CR number | it has a real `## CR-n —` section in the ledger — the registry cannot point at nothing |

---

## Judgement calls worth reviewing rather than accepting

1. **The freshness test FAILS when no upstream contract can be found**, rather than printing a warning
   and passing. Rationale: this repo's standing rule against silent skips — a missing `vendor` symlink
   once made conformance rows skip unnoticed — and a stale vendored schema validates every message
   against last week's contract while the suite stays green. `AETHER_CONTRACT_OPTIONAL=1` downgrades it
   to a warning, and `AETHER_CONTRACT_SCHEMA` points it at a non-standard checkout. The cost is that a
   clone without the sibling repo needs one env var.
2. **`send_raw` registers `id -> method` for hand-written requests.** This widens per-method result
   checking beyond `call()`-driven traffic to the many tests that write the request line by hand. Stated
   plainly: it has **not yet caught anything** — CR-14 came through `call()` (via `ok`), and CR-15's three
   failures are envelope-level and need no attribution at all. It is cheap and it closes a gap that would
   otherwise be invisible, but it is a widening on principle rather than one with a yield behind it, and
   the cost is that a test writing a deliberately odd request line gets its reply held to that method's
   schema.
3. **Both divergences are registered rather than fixed in the server.** D14 says the schema governs the
   wire until amended, which read literally would make CR-14 our change to make. It is not taken here
   because the change is amputating `truncated` from a list result, and F4's own reasoning applies:
   *conforming by amputation*. If the owner rules the other way it is a small edit in
   `Engine::lookup_symbol` plus deleting one registry entry and its allowance.
4. **CR-14's allowance re-checks the key against the house shape rather than skipping it.** That is
   stricter than a plain exemption and it is what keeps `truncated` protected while the ruling is
   pending — but it does mean the harness is, for that one key, asserting the *un-ruled* shape. The
   alternative (skip the key entirely) would leave the truncation flag unguarded for however long the
   ruling takes. Reviewable either way; this is the choice made and the reason.
5. **`check_incoming_strict` exists only for the liveness test.** It is not wired into `recv`, so nothing
   in the running suite ever sees the unallowed verdict. If a future reader wants a "how non-conformant
   are we really" run, `AETHER_STRICT=1` gating `recv` onto it would be a few lines — deliberately not
   added on spec.

---

## Postscript, 2026-08-15 evening: item 20 landed, and calls 3–5 above are all superseded

The contract ruled the surplus (`empyrean` `f309cc8`, `protocol.md` §11.5) and added **§8 item 20**:
*"A server's conformance suite MUST close every result against its schema fragment."* That changes three
of the five judgement calls above, and it is worth recording which way:

- **Call 3 is resolved, in our favour on the container and against us on the token.** CR-14's ruling
  adopted the bounded object (the server's shape *was* the better one) and struck the numeric
  `cursor`/`nextCursor` under new §2.4 clause (b). So there was no amputation to fear and no ruling to
  wait for: the entry is deleted, `rpc::bounded_array` stopped minting a token, and `checkpoint_list` —
  the one caller that can actually honour continuation — owns its own.
- **Call 4 is retired with the entry it described.**
- **Call 5 was answered by making the strict verdict the *only* verdict.** `AETHER_STRICT` was never
  built. Item 20 is a MUST, so closure is unconditional: every result is validated against its fragment
  composed with `unevaluatedProperties: false` (`common::schema::closed`). An env-gated strict mode would
  have made conformance opt-in, which is the sampling posture item 20 exists to end.

**What the closure found on its first run, beyond the two failures that were predicted:** exactly one
thing, and it is upstream. Five keys that §11.5's own prose registers by name never reached their schema
fragments — `initialize.limits`, `initialize.methodSummaries`, `read_memory.region`,
`read_memory.symbolDisp`, `read_memory.caveat`. Two fragments out of 22 were left behind by the amendment
that created item 20. Raised as **CR-16**, registered with per-key checkers that assert what §2.1/§2.4/§11.5
say those keys are, so the allowance swaps authorities rather than opening a hole.

That is the item's whole argument, measured on day one: three sampling passes each called their own count
a floor and each was wrong; the gate found the last five keys in a single run, and they were in the
document rather than in the server.

**The keyword's mechanics are now an executable assertion, not a note.**
`schema_conformance::the_strict_closure_rejects_a_surplus_key_and_needs_the_unevaluated_keyword_to_do_it`
compiles the same fragment both ways and asserts that `additionalProperties: false` rejects the
*conformant* reply on all four envelope fields while `unevaluatedProperties: false` accepts it and catches
the surplus — so nobody can "simplify" the harness to the obvious keyword without a red test explaining
why not.

---

## Files

| | |
|---|---|
| the validator | `crates/oracle-aether/tests/common/schema.rs` |
| the funnel it hangs off | `crates/oracle-aether/tests/common/mod.rs` (`Client::recv`) |
| coverage, registry liveness, controls, allowance fences | `crates/oracle-aether/tests/schema_conformance.rs` |
| the vendored schema + provenance | `crates/oracle-aether/tests/contract/` |
| the divergence registry | `common::schema::KNOWN_CONTRACT_DIVERGENCES` |
| the change requests | `docs/2026-08-14-aether-change-requests.md` (CR-14 — raised by the main session, confirmed here; CR-15 — new) |
| the behavioural half of item 13 | `crates/oracle-aether/tests/events.rs` |
