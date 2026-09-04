# CR-K — `emulator/status` can say the ROM is stale and cannot say the listing is

**Filed by:** oracle lane, 2026-09-04. **Grounds:** `F-RELOAD-KEEPS-STALE-SYMBOLS`, hit in the field
this session and reproduced as a wire test on `parcel/reload-symbol-freshness`. Every anchor below was
read firsthand in the vendored fragment and in the harness that enforces it, at `4f843e4`.

## The ask

**`emulator/status` should be able to say whether the listing it is resolving against is still the file
at `symbolsPath`** — one declared string key on that fragment, on the same footing and for the same
reason as the ROM-freshness answer a consumer already gets. No new method, no shape change to any
existing key, and nothing here asks a server to *do* anything differently: it asks the contract to
declare a place where "I cannot show you this table is current" is sayable.

Preferred spelling: `caveat` (a §2.4 string), which is what every other method in the catalog uses for
exactly this genre and which `status` alone among the state-reporting methods does not declare. A
structured `symbolFreshness` object mirroring the shim's `romFreshness` is the alternative and is
strictly more contract surface; this filing does not need it and does not ask for it.

## 1. What is actually happening

Measured this session, in this order, by a lane owner:

1. A peer lane published two new symbols and they were confirmed present in `s4.debug.lst` by grep.
2. `emulator/reload_rom` answered `reloaded: true`, **`symbolsDropped: false`**.
3. `emulator/lookup_symbol {name: "Level_Width"}` answered
   **`-32013 no symbol named or prefixed Level_Width`** — for a symbol sitting in the listing on disk.
4. A control lookup (`Player_Bound_Right`) resolved, which is the only reason step 3 was established
   as a real absence and not a broken probe.
5. An explicit `emulator/load_symbols` on the same path took the count 2963 → 2970 and the symbol
   appeared.

A session that read `symbolsDropped: false` and stopped there would have concluded the peer never
published their symbols, and the wrongness would have pointed at **their** landing.

## 2. Why the reply key was not the fix on its own

`reload_rom`'s reply now carries the verdict as a `caveat` (oracle
`parcel/reload-symbol-freshness`), which is legal under the fragment as it stands and needed no CR.
That closes the **reload** case and nothing else.

**The standing case has no home.** A session that connects once and runs for hours while the engine
lane rebuilds never calls `reload_rom` at all; its table goes stale under it with no event and no
observable. That is precisely the case the ROM half already answers — the shim's `romFreshness`
recomputes on **every `emulator/status`**, because staleness is a standing condition and not an edge —
and it is the case `status` cannot answer for symbols.

`status` already carries `symbolsPath` and `symbolCount`. It reports the two facts that would let a
client ask the question and gives it nothing to read the answer from.

## 3. The blockage, exactly

`emulator/status`'s result fragment declares, in full:

```
pc, sp, sr, frameToken, symbolCount, symbolAtPc, symbolDisp,
romBytes, romPath, symbolsPath, romLoading, display
```

plus `$defs/replyFields` (`droppedEvents` + the stamp). **No `caveat`. No `diagnostic`.**

§8 item 20's closure is enforced by `unevaluatedProperties: false` applied at the top level of every
result (`crates/oracle-aether/tests/common/schema.rs`, `fn closed`, and its own anti-vacuity control
`the_strict_closure_rejects_a_surplus_key_and_needs_the_unevaluated_keyword_to_do_it`). So a server
that answered this question on `status` — as a string or as an object — would be **non-conformant on
the wire**, and correctly so: an unknown key is a change request, never a shipment.

There is no legal spelling. That is the whole of this filing.

**And this is the second such row in a fortnight, which is the part worth a moment beyond this CR.**
`emulator/step`'s frame-budget shortfall was the first: a bounded server stopped early and its fragment
gave the reply no key to say so — `caveat` declared ABSENT there too — and it cost a full CR round-trip
(§11.33, CR-STEP-SHORTFALL, adopted 2026-09-04) before the truth could be told. Two independent
findings, one shape: **a method whose result fragment declares no free-text key cannot report anything
its designers did not foresee.** §2.4's `caveat` exists precisely for what a designer did not foresee,
so declaring it absent is a decision that the method will never have anything unforeseen to say. That
may be right for `emulator/sprites` (the precedent `step`'s `$comment` cites). It is demonstrably wrong
for the two state-reporting methods that have now needed it. Whether the ruling should be per-method or
a general rider on §2.4 is the contract's call, not this lane's — but the choice is now evidenced
rather than hypothetical.

## 4. Why this is a contract question and not a local decision

**The asymmetry is already published.** `status` declares `romPath` **and** `romBytes` — the pair a
consumer needs to check the loaded image against the file — and for symbols it declares `symbolsPath`
**and** `symbolCount`, which is not that pair: a count is not an identity. Two builds of the same
listing routinely share a count while disagreeing on addresses (of the symbols `s4.lst` and
`s4.debug.lst` share, 92.6% name a different address — measured, and recorded in
`Engine::load_symbols`'s own comment). So the ROM half of `status` is checkable by a client and the
symbol half is not checkable **by construction**, and the client cannot close that gap itself.

**The prose the contract already commits to points the same way.** §4/D7 is this document's most
emphatic decision, and its named hazard is a client resolving names against a table that no longer
describes what it thinks it describes. D7 is enforced today only at the two *edges* — `load_symbols`
refuses a mismatched listing, `reload_rom` drops one — and both edges use `validate_against_rom`,
which `load_symbols`'s own caveat already concedes is *"a filter, not a proof: Match means 'not
obviously wrong', never 'proven right'"*. A same-shape rebuild is exactly what that filter cannot see.
D7's hazard therefore has a standing form that the contract describes and gives no server a way to
report.

## 5. What a server would put there, and what it must not claim

The verdict this lane implemented, offered as the reference semantics:

| state | quiet? | meaning |
|---|---|---|
| the file at `symbolsPath` parses to exactly the rows held | **quiet** | the only silent state |
| it parses to different rows | loud | the listing has been rebuilt under us |
| it cannot be read | loud | *could not check* — never rendered as "fine" |
| it no longer parses as a listing | loud | *could not check* |
| a table is held with no recorded path | loud | *could not check at all* |

**The bound this filing asks the contract to hold the wording to:** the quiet state means *the file has
not moved past the table*, and never *the table is right for this image*. Those are different claims
and the second is not measurable from here. This is `load_symbols`'s own standard, applied to the
question one layer up — and it is why the recommended spelling is a `caveat` string rather than a
state word like `verified`, which invites the stronger reading.

## 6. Cost, since it is a per-`status` check

Re-reading and re-parsing the listing at question time is what makes the verdict a measurement rather
than a memory (a load-time digest cannot be supplied by three of this server's four symbol-load
routes). On `status` that is a parse per call, where `reload_rom` pays it once per reload. aeon's
`s4.debug.lst` is 357 KB / 6,690 lines, so the parse is cheap in absolute terms — but `status` is
polled, and a server may reasonably want to gate the re-read on the file's mtime and size before
paying for a parse. That is an implementation freedom, not a contract question, and it is named here
only so the ruling is not read as mandating a parse per poll.

## 7. What this filing does NOT ask for

* **No auto-re-read.** A reload that silently replaces the caller's listing substitutes our judgement
  for theirs — a caller may have deliberately loaded a listing that is not the one beside the ROM —
  and a re-read that fails leaves only two bad moves: keep the stale table silently (the defect) or
  drop it silently (a reload that ends with no symbols, worse). Rejected on the merits, not deferred.
* **No change to `symbolsDropped`.** It answers "did I drop them", it is REQUIRED present even when
  false (§2.3), and that is the one thing about it that is honest. The defect was never that it lied;
  it was that it was the only thing there to read.
* **No new method and no event.** The standing condition wants the polled surface, which is `status`.

Claude-Session: https://claude.ai/code/session_011yZyNEtCdBPMDhfWmw8sLM
