# Vendored contract artifacts — provenance

`bus-protocol.schema.json` in this directory is a **verbatim copy** of the Aether wire schema from the
contract repo. It is vendored, not read from the sibling checkout at test time, so the test suite is
hermetic: it compiles against a fixed schema and produces the same verdict on a machine that has no
`empyrean/` checkout at all.

The copy is not allowed to rot, and **since 2026-09-02 the way that is checked changed shape.** The gate
no longer reads a file out of the sibling checkout. It **hashes the vendored bytes and compares the hash
against the blob pinned below**, which is content-addressed, hermetic, and cannot be satisfied by a
coincidentally-similar file. Confirming that pin against the contract repo is a second, *optional* step
that runs only when a variable points at one, and says loudly in the run's own output when it did not.

The rule is the suite's, not this lane's invention: `empyrean/contract/SUITE_PATHS.md`, *"What a resolver
owes its reader"*, at `38f6df4` — **"A gate that proves a vendored copy of a peer's CONTENT is fresh reads
the peer through git objects at a named revision, never through the peer's working tree"**, citing this
repo's own finding `F-SCHEMA-READS-LIVE-EMPYREAN` by name. What this file therefore has to carry is the
pin itself; see [How the freshness gate resolves](#how-the-freshness-gate-resolves) below.

<!-- The three lines below are PARSED by tests/schema_conformance.rs. Keep the exact `key = value`
     shape; the test fails loudly (not silently) if a marker is missing or malformed. -->

    pin.revision = 8a9309194ce67144a2efb532323947f623b64f96
    pin.blob     = 41ec64709e9dd63e38b11b10f759ff4a3335410d
    pin.bytes    = 347476

## Current copy

| | |
|---|---|
| Source | `empyrean/contract/schema/bus-protocol.schema.json` |
| Contract repo revision | **`8a9309194ce67144a2efb532323947f623b64f96`** (2026-09-04) — *not* `origin/main`'s tip (`7498dd2` at adoption), and that is deliberate: it is the last commit that **touched this file**, derived with `git log -1 --format=%H origin/main -- contract/schema/bus-protocol.schema.json` rather than assumed from the tip, and `git merge-base --is-ancestor 8a930919 origin/main` was **run**, not assumed (it exited 0). 67 method fragments; all 67 declare `params`, all 67 close it with `unevaluatedProperties: false` (handshake exempt), and all 67 declare `result`; 19 `$defs` — every figure **re-derived by parsing this copy**, never carried over from the table this replaced. |
| Last commit that touched the schema | **`8a9309194ce67144a2efb532323947f623b64f96`** — *"protocol: section 11.33, CR-STEP-SHORTFALL adjudicated: stepped? on emulator/step's result (the instructions actually retired); four vectors; two mutations"* (2026-09-04). |
| Git blob | `41ec64709e9dd63e38b11b10f759ff4a3335410d` |
| SHA-256 | `9d065dcb6312eb825a009bea4d8b63b2fc592e92ab522385afc536d312954442` |
| Bytes | 347476 |
| Vendored on | 2026-09-04 |

> **⚑ The pin is `8a930919`, NOT §11.33's own adoption commit `3b43185e`, and that is the recipe rather
> than a preference.** `3b43185e` is the *correction* to §11.33 — it removes the `caveat` fail-vector that
> could not be one, since the harness closes pass vectors only — and it touches `contract/protocol.md` and
> `contract/schema/tests/vectors.json` **and not this file**. `git log -1 --format=%H origin/main --
> contract/schema/bus-protocol.schema.json` answers `8a930919`, and the two revisions resolve this path to
> **the same blob** `41ec6470…`, so pinning the tip-of-the-ruling would have named a commit that never
> wrote these bytes. Both were confirmed ancestors of `origin/main` by running
> `git merge-base --is-ancestor`, not by assuming.
>
> ⚑ **The table this replaces was STALE, and the stale half was the half no gate reads.** It described
> blob `487af407…` / revision `e04a94f…` / 345965 bytes while the pin block above said
> `cf40685488…` / `bd6af51a…` / 346137, and the vendored file hashed to `cf40685488…` at 346137 — so the
> **pin** was right and the **prose** was describing the re-vendor *before* last. `schema_conformance.rs`
> parses the `pin.*` markers and nothing at all reads this table, so the file was green while telling any
> human reader the wrong revision. Every figure in the table above is therefore derived by parsing the
> bytes actually written in this commit; none is carried over.

**Taken from the object store at a committed revision**, `git show 8a930919:contract/schema/bus-protocol.schema.json`, never copied out of the sibling working tree. The adoption was then checked **by content address**: `git hash-object` on the written file returns `41ec6470…`, equal to `git rev-parse 8a930919:contract/…`. That is the one check that cannot be talked into agreeing, and this repo has caught a doctored restore with it before.

## How the freshness gate resolves

Three steps, in order, and **the resolver prints which one answered before anything is checked against
it** (`SUITE_PATHS.md`: *"Say which step answered"*).

| step | source | what it proves |
|---|---|---|
| 0 | the vendored bytes themselves, hashed as a git blob and compared to `pin.blob` above | that this copy is the artifact the pin names. **Always runs. Never skipped.** This is the substance of the gate. |
| 1 | `$AETHER_CONTRACT_SCHEMA` — a path to a **file** | that the pinned bytes equal that file. The legitimate override, for a checkout with no peer repo, and for a nightly that wants to compare against something specific. |
| 2 | `$AETHER_CONTRACT_REPO` — a path to a **git checkout** of the contract repo, read through `git cat-file` / `git merge-base` and **never** through its working tree | that the pinned blob exists in that repo, and that `pin.revision` is an ancestor of its committed default branch — i.e. the pin names something real and merged, not a local draft. |
| — | neither variable set | **nothing about the peer**, said so in the run's output: the resolver prints a banner naming both variables and returns. There is no walk up the filesystem, which is precisely the shape `F-SCHEMA-READS-LIVE-EMPYREAN` registered. |

**What was given up, and why that was the trade.** The old gate walked up from `CARGO_MANIFEST_DIR`,
found `empyrean/contract/schema/bus-protocol.schema.json`, and byte-compared it — so a contract edit made
this suite red automatically. It also went red when a teammate saved mid-edit, and — the half that
matters — would have gone **green against a change no other lane could see**. The automatic alarm is now
the re-vendor discipline plus step 2 under a variable, and the honest reading is that a default local run
no longer notices upstream moving on its own. It notices a vendored copy being *edited*, which is the
failure the pin exists for, and it never again reports a verdict about somebody's desk.

**To re-vendor** (the whole recipe, and it never touches the working tree):

```sh
REPO=/path/to/empyrean
REV=$(git -C "$REPO" log -1 --format=%H origin/main -- contract/schema/bus-protocol.schema.json)
git -C "$REPO" merge-base --is-ancestor "$REV" origin/main   # must exit 0
git -C "$REPO" show "$REV:contract/schema/bus-protocol.schema.json" \
  > crates/oracle-aether/tests/contract/bus-protocol.schema.json
git hash-object crates/oracle-aether/tests/contract/bus-protocol.schema.json  # -> the new pin.blob
git -C "$REPO" rev-parse "$REV:contract/schema/bus-protocol.schema.json"      # must be identical
```

Then update the three `pin.*` markers and the **Current copy** table above, in the same commit as the
serve that needs the new schema.

*(Historical, kept because it records how an upstream drafting error is handled here — it belongs to the 2026-08-26 adoption, not to the current copy.)* **A drafting miss carried in the open, because it is upstream's and not ours.** The `a0c50a11` landing left one clause of the top-level `description` reading *"the eight BLOCKED rows print there too"* when the set had just become five. It was reported rather than patched locally — a vendored copy that is hand-corrected is a copy whose blob check is worthless — and the hub landed the fix as `55d99a68`, which is why **this** revision is the one adopted rather than `a0c50a11`. `contract/protocol.md` and `contract/schema/tests/vectors.json` are byte-identical at both.

> **Record the BLOB, not only the branch tip.** `origin/main` moved *twice while this re-vendor was
> running* — `7dad1e6a` when the source was first inspected, `9b46a235` twenty minutes later when the
> bytes were written — and at both tips `origin/main:contract/schema/bus-protocol.schema.json` resolves
> to the same blob `9d8cc3c3…`. A later reader re-resolving the *branch* gets whatever is current; a
> reader re-resolving the *blob* or `eecce95` gets what was actually adopted. The revision row above is a
> timestamp; the blob and sha256 rows are the artifact.

*(The unmerged-branch tracking box that stood here retired itself 2026-08-21 when `callers-amendment`
merged as `70c7bb4` — its third profiler-amendment carry, per its own recipe: copy from the object
store, never from the checkout.)*

### What this re-vendor adopted — §11.33, `stepped` on `emulator/step` (2026-09-04)

Landed **with** the serve, in one commit, for the reason the §11.32 box below states in general and this
row states in the sharpest possible form: the key is **optional**, so a re-vendor ahead of the serve is
green against a server that emits nothing, and a serve ahead of the re-vendor is red for a good reason —
`emulator/step`'s `result` is closed with `unevaluatedProperties: false`, so `stepped` on the wire against
the previous copy fails §8 item 20's closure. One commit is the only ordering with no window.

**The CR is this lane's own**, raised 2026-09-04 out of `emulator/step`'s handler doc, and the hub
adjudicated it under standing delegation, choosing `stepped` over the two alternatives the CR offered (a
`count` ceiling, or permission to carry `caveat`). This lane serves the SHOULD as well as the MUST: §11.33
requires the key only when it is short, and this server emits the retired count on **every** `step` reply,
because a client that cannot tell a bounding server from a non-bounding one cannot read absence at all.

Figures **re-derived by parsing the previous copy and this one**, never read from a commit message:

| | previous copy (`bd6af51a`) | this copy (`8a930919`) | delta |
|---|---|---|---|
| method fragments | 67 | **67** | unmoved — §11.33 is additive *inside* a row, not a new row |
| fragments declaring / closing `params` / declaring `result` | 67 / 67 / 67 | **67 / 67 / 67** | 0 / 0 / 0 |
| `$defs` | 19 | **19** | unmoved — `stepped` is an inline `integer`, not a new `$def` |
| `emulator/step.result` properties | `pc`, `symbol`, `symbolDisp` | **`pc`, `stepped`, `symbol`, `symbolDisp`** | **+1**: `stepped` (`type: integer`, `minimum: 1`, `description`) |
| `emulator/step.$comment` | ends at *"caveat is declared ABSENT (the sprites precedent)"* | **same, plus the §11.33 sentence** | 1 leaf changed |
| bytes | 346137 | **347476** | +1339 |

**What this re-vendor's green does and does not witness — and on this row the "does not" is most of it.**

* It witnesses that `stepped` is **declared**, which is the only thing the closure can prove: before these
  bytes, a reply carrying `stepped` was refused by `unevaluatedProperties: false`; after them it is
  accepted. That is a real, measurable flip and it is exactly one bit wide.
* It witnesses `minimum: 1`, so a server that emitted `stepped: 0` is red.
* **It cannot witness that the key is ever emitted.** `stepped` is optional and `required` names only
  `pc`, so a server that deleted the whole feature stays green here. `tests/step.rs` is the only thing
  that holds the emission, and it anchors on the machine rather than on the reply — see
  `a_bounded_step_reports_the_instructions_it_actually_retired`.
* **It cannot witness `stepped ≤ count`.** §11.33 says so itself: *"the schema cannot express that across
  params and result, and the gate does not check it, so the server must."* A cross-field invariant between
  a `params` object and a `result` object has nowhere to live in a per-method fragment. That obligation is
  held by `tests/step.rs` alone.
* **It cannot witness the `count` bounds either**, and that is the other half of this parcel. `minimum: 1`
  / `maximum: 1000000` on `count` arrived in the **previous** copy but one (§11.24, `0a4313e`, 2026-08-25)
  and this server ignored both for ten days while every gate stayed green — because a `params` fragment
  describes what a conformant **client sends**, and a server's duty to **refuse** what falls outside it is
  behaviour a document schema is structurally blind to. **A re-vendor is a silent contract change unless
  something checks the server against the new text.** The refusals are asserted from the wire in
  `tests/step.rs`, which is where that blindness is covered.

**One value the fragment cannot carry, reported rather than papered over.** `stepped` is `minimum: 1`, so
a `step` whose run retired **nothing** — a CPU already halted on `STOP` retires no instruction for the
whole frame budget — has no legal spelling: `0` is refused by the fragment and absence is the one reading
§11.33 says never means a shortfall. This server omits the key in that case, as the lesser of two wrongs,
and leaves `stopped`'s `deadlineReached` as the honest channel. Owed upstream as a `minimum: 0` amendment;
the handler carries the same note at the `if retired >= 1` that implements it.

### What this re-vendor adopted — §11.32, the three object MUTATION rows (2026-09-03)

Landed **with** `emulator/object_spawn` / `_move` / `_delete` in one commit, and the ordering inside the
parcel was the load-bearing part rather than the fact that both are present. Two gates go red the moment
either half lands alone, in opposite directions: `params_closure.rs` demands a fragment for every
advertised method (so the serve alone reddens it), and `schema_conformance.rs`'s coverage gate pins
`UNCOVERED_METHODS` empty (so the re-vendor alone would pass while nothing emitted the shape). That
second one is the trap: **a re-vendor ahead of the serve turns the gate green against a server that does
not emit the shape**, which this repo has measured before, and this file's §11.30 box records the same
defect wearing annotation instead of absence.

**What ran BEFORE these bytes were written, and it is what the re-vendor rule is actually for:** the
three rows were served, driven against a real aeon build carrying the mailbox, and every real reply and
every real error object was validated against the hub's fragments — the fragments read out of the object
store, not out of this directory. Six real replies passed the `result` fragment **closed with
`unevaluatedProperties: false`**; twenty-three real error objects passed `$defs/errorObject`; all six of
the hub's `expect: "pass"` params vectors were accepted by the running server and their replies passed
the closed result fragment; all sixteen `expect: "fail"` vectors were refused. Nothing the server emits
is refused by a fragment. That check is the lane's standing commitment (`docs/2026-09-02-cr-spawn-mode.md`
§17), and it exists because on CR-F one lane both authored and verified vectors and nine of eleven could
not have passed.

Figures **re-derived by parsing the previous copy and this one**, never read from a commit message:

| | previous copy (`82982b7`) | this copy (`e04a94f`) | delta |
|---|---|---|---|
| method fragments | 64 | **67** | **+3**: `emulator/object_spawn`, `emulator/object_move`, `emulator/object_delete` |
| fragments declaring / closing / resulting | 64 / 64 / 64 | **67 / 67 / 67** | +3 / +3 / +3 |
| `$defs` | 19 | **19** | unmoved — the rows reuse `hex`, `symbolName`, `decoderLayout`, `replyFields` |
| `$defs.errorObject.data.reason` | 6 named discriminants | **12** | +6: `objectPoolFull`, `unknownSlot`, `slotOwnedByEntityWindow`, `mailboxNotConsumed`, `frameMoved`, `mailboxLayoutUnexpected`. **Prose inside a `description`, not an enum** — §11.18's rule that an emitted enum is unwidenable — so this half carries **no validation force** and no green here witnesses a reason spelling. `tests/object_mutation.rs` asserts each one on the wire. |

**What this re-vendor's green does and does not witness.** The `params` half has real force: all three
fragments are closed, so `params_closure.rs` ties each row's accepted key set to the fragment by parse
and a drifted spelling is red. The `result` half has force for shape (required keys, `layout` closed
against `decoderLayout`) and none at all for the two things §11.32 spends most of its words on — that a
refusal by the game reaches the client as a **typed error** rather than a result field, and that `x`/`y`
are a **re-read** rather than an echo. A schema cannot see either: a server that returned
`{"status": 3}` in a result would fail the fragment only because `status` is an undeclared key at a level
the fragment does not close, and an echo and a re-read are the same JSON. Both are held at runtime by
`tests/object_mutation.rs`, whose consumer is a 68000 test double precisely so the machine's record can
disagree with the request.

### What this re-vendor adopted — §11.31, `stopPrecision` (2026-09-02)

Unlike the §11.30 re-vendor below, this one **has validation force**, and it is the reason the re-vendor
could not land before the serve: `events["emulator/stopped"].params.required` gained `stopPrecision`, so
the moment these bytes arrived, six hand-built `stopped` fixtures in `tests/schema_conformance.rs` went
red for exactly the right reason (`"stopPrecision" is a required property`) and so did every live event
the server emitted until the serve landed beside it. Both halves are in one commit for that reason.

Figures **re-derived by parsing the previous copy and this one**, never read from a commit message:

| | previous copy (`e7e94fa6`) | this copy (`82982b7`) | delta |
|---|---|---|---|
| method fragments | 64 | **64** | unmoved — no method added, renamed or removed |
| fragments declaring / closing / resulting | 64 / 64 / 64 | **64 / 64 / 64** | unmoved |
| `$defs` | 18 | **19** | **+1**: `stopPrecision`, one enum `$ref`d twice (the handshake map and the event), *"since a copied enum is a drift source"* |
| `handshake.initialize.result.properties` | — | **+ `stopPrecision`** | a closed object keyed by the eight `reason` values, `minProperties: 1`, **not** in `required` — presence is the amendment discriminator (§2.1 rule 2) |
| `events["emulator/stopped"].params.required` | `["reason","pc"]` | **`["reason","pc","stopPrecision"]`** | the one shape change with teeth |

**What the vendored schema still cannot check, so nobody reads its green as more than it is** (§11.31
says this itself): the binding rule (it relates two messages), the key-set rule's over-declared half, the
client-side absence rule, and the opt-in rule. The first and the key-set's under-declared half are held
at runtime by `tests/stop_precision.rs` (contract §8 item 24); the other two are prose obligations a
reviewer reads. Note in particular that `minProperties: 1` means **an under-declared handshake map is
schema-valid** — the map could name one reason out of seven and this schema would accept it. That gap is
why item 24's key-set assertion is not decorative.

### What this re-vendor adopted — §11.30, **three `description` edits and nothing else** (2026-08-30)

**⚑ Read this before trusting any green run that follows it.** §11.30 (CR-I, filed by this lane —
`docs/proposed/2026-08-30-cr-i-symbolspath.md`, adopted at `e7e94fa6`) is the first re-vendor in this
file's history whose entire schema delta is **annotation**. Three `description` strings changed. JSON
Schema `description` has **no validation force**, so the moment these bytes landed,
`schema_conformance.rs`'s byte-identity gate went green and every conformance vector still passed —
against a server that was still putting a relative `symbolsPath` on the wire. **The re-vendor's green
witnesses nothing about the behaviour it describes.** What witnesses it is
`crates/oracle-aether/tests/symbols_path.rs`, which is behavioural, was red-first against this repo at
`5808d8c`, and asserts the actual spellings. This box is here so the next reader does not have to
rediscover that the gate and the subject are disjoint.

Every figure below is **re-derived by parsing the old copy and the new one** (`git show
HEAD:…/bus-protocol.schema.json` against the written file), never read from a commit message.

| | previous copy | this copy | delta |
|---|---|---|---|
| method fragments (`methods`, `$`-keys excluded) | 64 | **64** | **unmoved** — no method added, renamed or removed |
| fragments declaring `params` | 64 | **64** | unmoved |
| fragments closing `params` with `unevaluatedProperties: false` | 64 | **64** | unmoved |
| fragments declaring `result` | 64 | **64** | unmoved |
| `$defs` | 18 | **18** | unmoved |
| top-level keys outside `methods` | — | — | **byte-identical subtree**, checked by serializing both with sorted keys |
| fragments otherwise changed | — | **3** | `emulator/status`, `emulator/load_symbols`, `emulator/screenshot` |
| `UNCOVERED_METHODS` | 0 | **0** | unmoved |
| `SCHEMATIZED_NOT_ADVERTISED` | 8 | **8** | unmoved |
| bytes | 320558 | **321300** | +742, all of it prose |

**The delta was enumerated leaf by leaf rather than eyeballed from a diff**, by walking both documents to
their scalars and comparing every path. **Total leaf differences: 3.** In full:

* `/methods/emulator/status/result/properties/symbolsPath/description`
* `/methods/emulator/load_symbols/result/properties/path/description`
* `/methods/emulator/screenshot/result/properties/path/description`

And the complementary check, which is the one that actually rules out a shape change hiding in the
prose: with **every** `description` in both documents stripped recursively, the two parse to the *same
value*. So no `type`, `required`, `enum`, `$ref`, `additionalProperties` or
`unevaluatedProperties` moved anywhere in the file.

**What the three edits say.** `symbolsPath` no longer reads *"Path to the loaded listing, same
treatment"* — the gesture at its neighbour that CR-I identified as the cause of the divergence, since it
reads both as *same trust model* (D8) and as *same handling* (§6) and a server can satisfy one while
violating the other. It now names the resolution outright. `load_symbols.result.path` and
`screenshot.result.path` gain descriptions saying the same, and `load_symbols`'s carries M1 in words:
*"One method never reports one file under two spellings in one exchange."*

**A SHOULD is not a schema fact, and the ruling says so.** §11.30: *"the fragments constrain type, and a
SHOULD is not a schema fact, so the gate stays as it was and a conformance run cannot certify this; a
server that reports a raw path is non-conformant on §6's SHOULD, visible only by reading the reply."*
That is upstream stating this box's point in its own adjudication.

### What this re-vendor adopted — §11.29, 63 → 64 fragments (2026-08-30)

**One fragment added and one existing fragment amended, and this repo serves both in the same parcel.**
§11.29 (CR-H) adds `emulator/screen_text` — the text a human can read on the player window, source string
and rendered prefix both — and gives `emulator/status` an optional additive `display: boolean` so a caller
can *ask* whether a window exists rather than probing by provoking a refusal.

Every figure below is **re-derived by parsing the old copy and the new one**, never read from a commit
message, and the diff is `89 insertions(+), 0 deletions(-)`.

| | previous copy | this copy | delta |
|---|---|---|---|
| method fragments (`methods`, `$`-keys excluded) | 63 | **64** | **+1**: `emulator/screen_text` |
| fragments declaring `result` | 63 | **64** | +1, the same name |
| fragments closing `params` with `unevaluatedProperties: false` | 63 | **64** | +1, the same name |
| `$defs` | 18 | **18** | **unmoved** — the new fragment reuses §2.4's FLAT bounded-list spelling rather than adding an envelope |
| fragments otherwise changed | — | **1** | `emulator/status`, one optional additive `display` key |
| top-level keys outside `methods` | — | — | **unchanged**, byte for byte |
| `UNCOVERED_METHODS` | 0 | **0** | unmoved — the fragment and the handler land together |
| `SCHEMATIZED_NOT_ADVERTISED` | 8 | **8** | **unmoved**, for the same reason |

**Nothing else rode in.** That is worth stating because the previous re-vendor commit *appeared* to be
three revisions behind: the `## Current copy` block above still named `55d99a68` while the vendored bytes
had been moved forward three times since (by `2823b22`, `cfaf6c0` and `0f35ae1`, which re-vendored the
schema without touching this file). The bytes were checked rather than the prose trusted — the vendored
copy's sha256 was `a7a9754c…`, which is `empyrean` `ec008ec`'s blob, the commit immediately before
§11.29 — so the adopted delta really is CR-H and the rider alone. **The lesson is the one this file already
teaches one paragraph up: the revision row is a claim, the blob and sha256 rows are the artifact. A
re-vendor must move both.**

**`SCHEMATIZED_NOT_ADVERTISED` does not move, and that is a decision rather than an absence.** The
adjudication attached a condition — *"the fragment is SCHEMATIZED AHEAD OF BEING SERVED; do not advertise
the method until your vectors, derived from a real reply, land at empyrean"* — which reads like an
instruction to park `emulator/screen_text` in that pin. It is not, and the pin's own second bullet says
why: a name sits there because **this server does not serve it**, and a vector cannot be derived from a
real reply without a real handler. So the handler ships on this branch, the row joins `METHODS`, the pin
stays empty of it, and the *advertising* is held at the merge instead — where holding it costs nothing and
proves something.

### What this re-vendor adopted — §11.25, 59 → 62 fragments (2026-08-26)

**Three fragments added, and this repo serves all three in the same commit.** §11.25 (CR-D) is the first
amendment to *remove* rows from the schema's BLOCKED set rather than add rows to the catalog: it gives
`emulator/object_list`, `emulator/player_state` and `emulator/object_slot` a shape — a closed envelope
over a typed-open `fields` payload with a REQUIRED `layout` discriminant — and closes audit D-27 for all
three. Both figures below are **re-derived by parsing the old copy and the new one**, never read from a
commit message.

| | 59-fragment copy | this copy | delta |
|---|---|---|---|
| method fragments (`methods`, `$`-keys excluded) | 59 | **62** | **+3**: `emulator/object_list`, `emulator/player_state`, `emulator/object_slot` |
| fragments declaring `result` | 59 | **62** | +3, the same names |
| fragments closing `params` | 59 | **62** | +3, the same names |
| `$defs` | 15 | **17** | **+2**: `decoderLayout` (closed), `decodedSlot` (deliberately **unclosed** and carrying **no `required`** — a shape library, per M2/M3) |
| `limits` declared keys | 10 | **11** | +1 **optional** `maxObjectSlots`; `limits.required` unchanged |
| §6 rows the description calls BLOCKED | 8 | **5** | −3, the same names — `z80_registers`, `read_vdp_registers`, `read_vsram`, `call_stack`, `log_tail` remain |
| `UNCOVERED_METHODS` (advertised, no `result` fragment) | 0 | **0** | unmoved — the three arrived schematized *and* advertised in one commit, so neither pin ever saw them |
| `SCHEMATIZED_NOT_ADVERTISED` | 18 | **18** | **unmoved**, for the same reason |

Both pins staying put is the finding, not an absence of one. The usual shape is a fragment landing ahead
of its handler (`SCHEMATIZED_NOT_ADVERTISED` grows, then shrinks when the handler ships); here the
handler and the fragment arrive together, so the set that would have held them is empty at both ends and
the two assertions that guard it were satisfied by re-derivation rather than by editing a number.

**`capabilities.objectDecoders` keeps its boolean type** and gains a pinned meaning: *this build has the
handlers*, `true` iff **at least one** of the three rows is in `methods` (§8 item 23 keeps per-row
servedness as `methods` membership). This server now publishes `true`, derived from `METHODS` rather
than typed, so the flag cannot disagree with the list.

**`limits.maxObjectSlots` is deliberately not emitted.** The key is optional and its absence is
meaningful: a server that applies no policy ceiling omits it, and `object_list.limit` is then bounded
only by `layout.slotCount` — which is what this server does, and what its `limit` refusals are measured
against.

**No deviation is outstanding at this commit.** The `step_over`/`step_out` `pc` shortfall the previous
re-vendor recorded was closed before this one (the handlers now report the halt they compute), and the
whole aether suite is green against these bytes: 30 legs, 360 passed, 0 failed, 2 ignored. Every
server→client line is validated against `methods.<name>.result` closed with `unevaluatedProperties:
false`, so that green covers the three new rows' replies — proven by mutation rather than assumed, in
both closure forms: emitting a stranger key on an `object_list` item is refused by the item's
`additionalProperties: false`, and the same key on the `object_slot` **result top level** is refused by
item 20's harness-side `unevaluatedProperties`, which is flag 2's third-use-site distinction arriving on
a real reply. The `active: false` conditional bites too: a reply carrying `x` beside `active: false` is
refused by the `else` branch's `anyOf`, which is M2 and the delta's M7 checked against a running server.

### History — the §11.21 – §11.24 re-vendor, 58 → 59 fragments (2026-08-26)

Four amendments arrived together, because the vendored copy had not moved since 2026-08-22 and the
contract had. **Exactly one method fragment was added across all four** — every other change is to an
existing fragment or to the handshake — and that asymmetry is the whole of the pin arithmetic below.
Both fragment sets were re-derived by parsing the old copy and the new one, never read from a commit
message.

| | 58-fragment copy | this copy | delta |
|---|---|---|---|
| method fragments (`methods`, `$`-keys excluded) | 58 | **59** | **+1**: `emulator/breakpoint_set_enabled` |
| fragments declaring `result` | 58 | **59** | +1, the same name |
| fragments closing `params` | 58 | **59** | +1, the same name |
| `UNCOVERED_METHODS` (advertised, no `result` fragment) | 0 | **0** | unmoved — the new fragment is for a method this server does not advertise |
| `SCHEMATIZED_NOT_ADVERTISED` | 17 | **18** | **+1**, the same name |

- **§11.21 (CR-BP, breakpoints)** is the only entry that added a row: `emulator/breakpoint_set_enabled`,
  plus a `handle` shape on `breakpoint_add`/`_list`/`_clear`, an `unknownBreakpoint` /
  `breakpointCapReached` pair in the `-32005` `reason` prose, and a `breakpoint` handle on the `stopped`
  event, conditional on `reason == "breakpoint"`. This server publishes `capabilities.breakpoints:
  false` and advertises none of the five rows, so the addition lands in `SCHEMATIZED_NOT_ADVERTISED`
  and the pin moves 17 → 18.
- **§11.22 (`z80_write` byte rule, setter enums)** changed `z80_read`/`z80_write`,
  `set_layer_enabled`/`set_channel_enabled` and `vgm_status`. **No fragment added.** Every one of those
  names was already in `SCHEMATIZED_NOT_ADVERTISED`, so no pin moves.
- **§11.23 (CR-C, server identity)** — the entry this parcel serves — changed **only the handshake**:
  `implementation` and `serverBuild` as new `properties`, three names appended to
  `initialize.result.required` (`serverVersion` among them, promoted), and descriptions on `serverName`
  and `serverVersion`. **No fragment added, no pin moved.** Its cost lands on the server instead: the
  suite compiles `handshake.initialize.result` **closed**, so the three newly-required keys are a hard
  red until the emission ships — which is why the emission and this re-vendor are one commit.
- **§11.24 (audit batch B1)** changed `ping`, `step`, `step_over`, `step_out`, `wait_for_break` and
  `breakpoint_*`. **No fragment added.** It is the only one of the four with a live consequence for a
  row this server *does* serve — see the deviation recorded below.

**A deviation this re-vendor adopts knowingly: `step_over` and `step_out` now owe `pc`.** §11.24 closed
audit D-03 by giving both rows the same result as `emulator/step` (`pc` REQUIRED, `symbol?`,
`symbolDisp?`); this server returns `{}` from both (`engine.rs`'s `step_over`/`step_out`), which was
transcribed *from* the pre-amendment fragment and is now short of the contract. The handlers are **not**
changed here: §11.24's behavioural asks are a separate parcel, and inventing the fix inside a re-vendor
commit is how a schema quietly becomes a record of what a server does. It is recorded, not silenced, and
it is the reason the aether suite is not fully green at this commit.

### History — the §9 mechanical-completion pass, 37 → 58 fragments (2026-08-22)

**Twenty-one fragments added, none removed, and not one existing fragment changed content.** All three
figures re-derived by parse rather than carried from the upstream commit message: the 37 pre-existing
fragments, plus `$defs`, `anyMessage`, `events` and `handshake`, compare **structurally identical**
between the two revisions by parsed-JSON equality. The adopted surface is **purely additive**, and the
whole of the addition is methods this server does not serve.

The 21 — `ping`, `step`, `step_over`, `step_out`, `run_to_scanline`, `wait_for_break`, `z80_read`,
`z80_write`, `breakpoint_add`/`_list`/`_clear`, `write_vram`, `set_layer_enabled`, `get_layer_states`,
`set_channel_enabled`, `get_channel_states`, `vgm_start`/`_stop`/`_status`, `audio_spectrum`,
`log_clear` — are **§6 catalog rows, every one written from its row and none from a server's replies**,
which is §8's required direction. Eight §6 rows remain deliberately unschematized (`z80_registers`,
`read_vdp_registers`, `read_vsram`, `object_slot`, `object_list`, `player_state`, `call_stack`,
`log_tail`), each because its result is stated too loosely to transcribe without inventing.

**None of the 21 is reachable in this process.** `oracle-next` advertises 37 methods; every one of the 21
answers `-32601 no such method` on every route — there is no `METHODS` row, no handler symbol, no cargo
feature (this crate has no `[features]` section), and no runtime toggle. Eight are governed by a
capability flag this server already publishes as `false`. The 2026-08-22 dry run
(`docs/2026-08-22-schema-fragment-dryrun.md`) enumerated all seven routes that could otherwise produce a
reply and named the blocker on each; the adoption's own findings are in
`docs/2026-08-22-revendor-58.md`.

**Adopting it moved no verdict on the surface we serve.** The whole 24-leg aether suite ran with these
bytes in place and **not one reply this server emits was refused by any fragment** — measured, not
inferred, since every server→client line funnels through `Client::recv` and is validated against
`methods.<name>.result` closed with `unevaluatedProperties: false` (§8 item 20).

**Three checks in this repo went red, and all three were tests encoding assumptions the 58-fragment set
legitimately invalidates.** Each was reshaped on its merits and each reshape was proven by making it fail
on purpose first:

- **The decision this re-vendor owed** (`schema_conformance.rs`). `assert!(schema_only.is_empty())` was
  written with the rule that a fragment landing ahead of its handler "has to be a decision, taken in the
  commit that re-vendors". This is that commit. **Ruled: schematized-but-unadvertised is a legitimate
  steady state, and these 21 are not deferred work** — §8 item 20 makes the fragment the precondition for
  a handler and not its record, and §6 is the suite catalog rather than our backlog. The assertion became
  a **pinned set of the 21**, not a printed count: a count reports `22` and stays green on the next
  arrival, and both a bare count and `is_empty()` are satisfied by a schema that failed to load. A pin of
  21 names fails on an unparsed document, on a 22nd arrival, and on one of the 21 becoming served.
- **The description's fragment count** (`params_closure.rs`). Upstream did not repair `37` to `58`; it
  **deleted the number** and pointed the prose at its own parsing gate, on the reasoning that a correction
  carrying the coordinate inherits the defect. Our check demanded a count exist and read the *first* of
  several. It now checks **every** stated count (the pre-fix candidate stated two, `37` and `58`, and a
  last-match parser would have gone green on a self-contradicting document), and where none is stated it
  requires the document's own deliberate-omission disclaimer — so "no count" is measured rather than
  assumed.
- **D-33, the wire-spelling divergence** (`mcp_tool_sweep.rs`). The new `audio_spectrum` and
  `wait_for_break` fragments made a latent conflict measurable for the first time: the legacy MCP client
  sends `fft_size`, `max_hz`, `timeout_ms` where §6 spells `fftSize`, `maxHz`, `timeoutMs`. The legacy
  *server* reads the snake_case spellings too, so client and server agree with each other and both diverge
  from §6 — "fix the client" would have broken working tooling. empyrean ruled direction only (camelCase
  stands, §6 does not move) and left the migration, which must move server and client together, as the
  owner's call: *"Nothing in the ruling changes code today."* Registered here rather than fixed, with the
  registry's own claim — that each entry is a *respelling* whose camelCase partner the fragment declares —
  re-derived from the schema, so the registry cannot be used to hide a genuine client bug.

### What this re-vendor adopted — §11.18 (CR-28), the caller lens

**No fragment is added and none is removed: 37 before and 37 after**, re-derived by parsing both revisions
rather than carried forward (§11.17 clause 7). The movement is **nineteen newly declared properties** inside
three fragments that already existed, plus the `initialize.limits` key that signals the lens exists.

- **`initialize.limits.maxProfilerCallers`** — the largest `topCallers` accepted and the ceiling applied to
  each row's `callers` list. **Its presence IS the capability signal**: a server implementing the lens MUST
  advertise it, a server without the lens MUST omit it. A **reply** bound, not a retention bound — the
  accumulator keeps every observed edge, which is what makes a row's `callersTotal` the true count of
  distinct callers rather than the count that survived a ceiling. Refused above, never clamped.
- **`emulator/set_profiler`** gains the `callers` param (opt-in, default false, resets with every arm) and a
  `callers` echo in the result. The echo is **conditional, not REQUIRED**: §11.16's pre-release licence
  expired when the profiler arc merged and this server shipped `"profiler": true`, so absence means *this
  server has no caller lens* and `false` means *the lens exists and is off*.
- **`emulator/get_profiler`** gains the same conditional `callers` echo — the third arming fact. It reports
  the instrument's **state** and carries no rows, so `callersNotArmed` is structurally unreachable there.
- **`emulator/get_profiler_frames`** gains the `topCallers` param and four row keys that arrive **as a
  set** — `callers`, `callersTotal`, `callersReturned`, `callersTruncated` — tied by `dependentRequired` so
  a half-served lens cannot pass the fragment. An edge is
  `{callerAddr?, callerName?, callerDisp?, entryKind?, cycles, cyclesSelf, calls, cyclesTotal,
  cyclesSelfTotal, callsTotal}` with `additionalProperties: false`, so **`stallCycles` on an edge is barred
  outright** — the requesting client declined one on measured grounds, and the fragment records the decision
  rather than leaving the key undeclared.

Three things about that shape are structural rather than stylistic, and each is pinned by a test here:

- **The `entryKind` biconditional** — REQUIRED exactly when `callerAddr` is absent, forbidden when it is
  present — is enforced by an `if`/`then`/`else` on the edge shape, not by prose. The enum is
  **four** values (`hint`, `vint`, `root`, `depthCap`); the collapsing spelling `"interrupt"` the demand side
  asked for was overruled and is refused by the enum. `tests/profiler.rs::the_entry_kind_biconditional_
  is_enforced_in_both_directions` and `::each_entry_kind_is_accepted_and_the_collapsing_spelling_is_not`.
- **Two normative sums**, both `==` and both guarded by `callersTruncated: false`: the edges' `callsTotal`
  sum to the row's, and their `cyclesSelfTotal` sum to the row's. Undivided on both sides of both, which is
  what makes them assertable with `==` rather than relaxable to a bound — §11.16's *quiet gap* argument at a
  smaller denominator. The undivided partners on the edge (`cyclesTotal`, `cyclesSelfTotal`, `callsTotal`)
  were **folded in at adjudication** rather than deferred, which is why `F-PROFILER-EDGE-UNDIVIDED` is
  retired and carries no debt.
- **The §2.4 spelling is FLAT, scoped to the item.** `callers` is not a nested `{items,total,…}` container:
  §2.4 gained a third case at this amendment for a list that is a field of an *item* of a container, and
  `routines.items[].callers` is its registered example. The three companions ride as **prefixed siblings**
  of the row.

**And the reply a client never armed is byte-identical to the pre-amendment one** — the entry's central
claim, and the one an always-on accumulator would break first. Pinned by
`tests/profiler.rs::an_unarmed_reply_is_byte_identical_to_a_never_armed_servers`.

### What the previous re-vendor adopted — the §11.17 postscript

**Two properties struck from one fragment: `emulator/reload_rom`'s `wait` and `reset`.** The fragment
count does not move (37 before and after) and no other shape changes.

Both were bare `{"type": "boolean"}` entries with **no description at all**, inherited from the legacy
catalog and never specified. **We** read `path` and nothing else, so here they were the silent-ignore
§2.5 exists to end — but the *legacy socket server* implements both for real
(`ControlSocket.cpp:1363`/`:1374`). Two servers, one with a behaviour, and a catalog describing neither:
a client could not have learned what to send, and one that guessed right on one server guessed wrong on
the other.

Struck rather than specified — writing the semantics down would adopt one implementation's choices as
normative on the strength of that implementation, which is a change request and not a postscript. They
are now refused by name like any other undeclared key
(`tests/params_closure.rs::reload_rom_refuses_the_two_struck_keys`).

**Our first draft of this said "no server had ever read either", which was false** — true of the
reference server, false of the legacy one. Caught by grepping the legacy source rather than by reasoning
about it, and corrected in the contract before the postscript settled. The register's own standing rule:
a claim falsifiable by execution does not belong in normative text.

**This is also the first re-vendor the authority test forced.** `params_closure.rs::every_advertised_
method_declares_exactly_its_fragments_params` compares `MethodSpec.params` against the fragment by parse,
so striking the keys upstream turned the suite red until the table matched. Table and schema cannot move
independently, which is the property that test exists for.

### What the previous re-vendor adopted — §11.16 (CR-26) delta 3, the undivided set

`4fc1915` adds **four REQUIRED integers to the routine row and four to the interrupt bucket** —
`cyclesTotal`, `cyclesSelfTotal`, `stallCyclesTotal`, `callsTotal` — the same four quantities the divided
figures report, over the whole sample, **undivided**. The divided figures stay REQUIRED and unchanged;
division-inside remains a pinned property of this surface. **The fragment count does not move** (36 before
and after), and no `initialize.limits` key is added.

Two consequences, and the second is the headline:

- **Each pair is tied**: when `frameCount > 0`, `divided == total / frameCount` under integer division, so
  `divided × frameCount ≤ total < (divided + 1) × frameCount` — a total *bounds* its partner's truncation.
- **The reconciliation identity gets a wire form that is unconditionally exact**:
  Σ `routines[].cyclesSelfTotal` + Σ `interrupts[].cyclesSelfTotal` + `unattributedCycles` ==
  `sampleCycles`, with no `perFrameExact` condition, no `× frameCount` and no floor bound. Delta 2 could
  only offer the divided reconstruction, hedged three ways; that hedging is now a property of the divided
  *view* alone. `tests/profiler.rs::the_identity_closes_when_computed_from_the_wire` asserts the exact form
  unconditionally and keeps the divided bound beneath it as the secondary check.

**And the four are carried on the routine row and the interrupt bucket only** — never on `perFrame[]` rows,
which are whole-frame totals with no per-routine breakdown and are already undivided. That bound is a
negative control here rather than trusted prose:
`tests/profiler.rs::the_undivided_set_is_refused_on_a_per_frame_row` doctors a real reply four ways and
asserts the fragment rejects each.

The ask this answers is the demand side's C2 — a per-frame `calls` is the one figure division routinely
*destroys* rather than merely truncates (4.53 invocations a frame reports `2`, one invocation across the
sample reports `0`), so no rate in their packet could be gated with `==` against it. The controller took
the whole undivided set rather than `callsTotal` alone because the pre-release window shuts once, and a
field registered for later can only come back optional-forever or in a v2.

### What the previous re-vendor adopted — §11.16 (CR-26) and its first two deltas

The profiler family's **first three fragments — 33 → 36** — plus two `initialize.limits` keys. This is an
**amendment** to three rows the catalog had carried since its first draft, not a new family: the methods
were advertised in prose with summarised results and no fragment at all, which is exactly the gap §8 item
20 exists to close.

- **`emulator/set_profiler`.** Arms or disarms the accountant. Arming **resets** the accumulators (no
  resume in this revision, so a second arm discards an in-flight sample); disarming **retains** the sample
  so it can still be read. The arm is synchronous with the reply, and none of the three methods may be
  refused `-32005` **for the machine's run state** — the sample's edges are frame boundaries, not the
  instant the command landed. `perFrame` is the opt-in per-frame ring.
- **`emulator/get_profiler`.** The instrument's state, not its data. `framesRecorded` is the SAME number
  `get_profiler_frames` calls `frameCount`, and the two MUST agree when no frames ran between the calls —
  the legacy surface had two counts that could differ and only one was ever the divisor.
- **`emulator/get_profiler_frames`.** The sample. Nine REQUIRED result keys (`frameCount`, `sampleCycles`,
  `totalCycles`, `unattributedCycles`, `abandonedFrames`, `depthExceeded`, `perFrameExact`, `routines`,
  `interrupts`), `budgetPct` **XOR** `budgetPctOmitted` enforced by an `anyOf` + `not` on the result, and
  the opt-in `perFrame` container. `routines` and `perFrame` take §2.4's **nested** container spelling and
  carry **no cursor** (clause (b)); `top` and `frames` are **refused, never clipped**, and `frames` without
  the ring armed is `-32005 perFrameNotArmed` — a refusal about the *instrument's* state, which the run-state
  exemption above does not touch.

**The delta the second commit added** (`64fc3f8`, `6d5cb4b`): `unattributedCycles`, `abandonedFrames` and
`depthExceeded` become REQUIRED result fields, and `cyclesSelf` becomes the interrupt bucket's fourth
REQUIRED field. That last one is the D-M1 fix and it is worth reading twice: §6 told a client to sum
`interrupts[].cyclesSelf` to check the reconciliation identity, while the bucket shape was closed and did
not carry the key — the contract directed a computation and then rejected every reply that permitted it.
**The fragment count does not move** (36 before and after); no other fragment is touched.

### One harness change that re-vendor forced

`get_profiler_frames` is the first fragment to define a **fragment-local `$defs`** (`interruptBucket`, so
`hint` and `vint` are provably one shape rather than two that can drift) and to reference it by an
**absolute in-document pointer**. Both are correct in the document they were written for; both break the
harness's lift-a-fragment-and-compile-it strategy, which clobbered the local `$defs` with the root's and
left the pointer with nothing to resolve against. `tests/common/schema.rs::with_defs` now merges root
`$defs` **under** a fragment's own and, only for fragments that use such a pointer, carries `methods` along
as an inert data key for the pointer to land on. The contract was not changed to suit the harness.

### What the previous re-vendor adopted — §11.14 (CR-24)

One new method fragment, taking the schema from **32 method fragments to 33** (the description string's
count is recounted again, 2026-08-18, §11.14; `methods` now holds 34 keys, one of them a `$comment`).

- **`emulator/scanlines` (CR-24).** Row-range readback of the most recently **completed** frame's rendered
  active display — the live raster with S/H applied and mid-frame CRAM/scroll effects included, never a
  post-hoc state render when a completed frame exists. Params `startLine`? (0–223, def 0) and `count`?
  (≥1, def: through line 223); `startLine`+`count` past 224 is `-32602`, refused never clipped, and the
  bound is structural (NTSC V28 active lines) so the list takes neither `truncated` nor a cursor. Result
  carries `startLine`, `mode` (`h40`/`h32`), `source` (`raster`/`stateRender`, `screenshot`'s spellings)
  and `rows[]{line,width,rgb}`, with `caveat` on the stateRender fallback only. The `mode` ↔ `rows[].width`
  ↔ `rgb`-length tie is **mechanical**: an `if`/`then`/`else` in `result.allOf` pins 320 px / 1920 hex
  digits against 256 px / 1536, with the loose `^0x([0-9A-Fa-f]{6})+$` whole-pixel pattern kept on the
  property as the floor. The `caveat` ⇔ `source` tie is deliberately left mechanically unenforced, matching
  `screenshot`'s fragment; the decision is recorded in the fragment's `$comment` rather than silently taken.

Schematized but not yet advertised by the reference server *at vendor time* (the handler landed two commits
later, so on the merged tree the method IS advertised), like §11.13's three: `tests/schema_conformance.rs`'s
`UNCOVERED_METHODS` stays empty (that list is for advertised methods missing a fragment, the opposite gap).

### What the previous re-vendor adopted — §11.13 (CR-21, CR-22, CR-23)

Three new method fragments, taking the schema from 29 method fragments to 32 (method count in the
description string goes from 23 advertised-with-result to 32 schematized total, recounted 2026-08-18):

- **`emulator/write_memory` (CR-21).** The poke primitive. Work-RAM window `$E00000-$FFFFFF` only, refused
  (`-32004`) never clipped; exactly one payload spelling (`bytes` XOR `value`+`width`, else `-32602`);
  requires a paused machine per the §6 run-control state rule (`-32005 machineRunning`). Values land
  big-endian. Never offered to the watch surface (a poke has no `pc` to name). New `limits.maxWriteLen`.
- **`emulator/reset` (CR-22).** A result was finally defined for a row that predated the schema. NOT subject
  to the run-control state rule — it replaces state wholesale between frames rather than advancing it, so it
  cannot fight the free-run loop, and MUST NOT change the machine's run state. New `deferred` boolean:
  `false` if applied before the reply was composed, `true` if handed off and bounded to land later (a server
  that cannot bound it answers `-32010` instead). The master clock restarts at 0.
- **`emulator/memory_hash` (CR-23).** Fingerprints a byte range without moving it — the gap `state_hash`
  leaves (its five hashes cover VDP state only). A pure read, never refused on a free-running machine.
  Two regions: work RAM (mirror-masked) or cartridge ROM; `-32004` for a base in neither or a range crossing
  out of its region. Returns both `fnv1a64` (`$defs/hash64`) and a new `crc32` (`$defs/hash32`, new
  `^0x[0-9A-Fa-f]{8}$` primitive) so a cartridge-window hash equals CRC32 over the same ROM-file slice. New
  `limits.maxHashLen`. Distinct from §9's deferred `frame_hash`.

All three are schematized-but-not-yet-advertised by the reference server — `tests/schema_conformance.rs`'s
`UNCOVERED_METHODS` stays empty (that list is for advertised methods missing a fragment, the opposite gap).

### What the re-vendor before that adopted — CR-9, CR-11 and CR-12

Two contract commits, both ruled in `docs/2026-08-15-fable-ruling-cr9-cr11-cr12.md`, taking the schema from
22 method fragments to **26** (62,434 → 89,020 bytes). *(`methods` holds 27 keys; one of them is a
`$comment`, which is where a "27" in a hand-count comes from.)*

- **`8adf219` — §11.7, CR-9.** `emulator/stopped` gains **`buttons`** and **`port`**, REQUIRED when the
  advance was driven by `emulator/press` and absent otherwise. The `reason` enum is **not** extended: §3
  redefines `runFrames` as *"a bounded frame advance ran to completion — `emulator/run_frames`,
  `emulator/press`, or any future method whose stop condition is an exhausted frame count"*, and pins the
  house rule that `reason` names the **condition**, never the method or the cause.

  **The enforcement is deliberately asymmetric, and the schema says so in a `$comment`.** The event carries
  no method discriminator — that is the *point* of the widening — so "present iff `press` drove it" cannot be
  keyed on an `if`/`then`. What is enforceable is enforced: `dependentRequired: {buttons:[port],
  port:[buttons]}`, because a subscriber told which buttons went down and not which pad would attribute the
  input to the wrong controller in a two-pad session. The behavioural half is ours to honour and is pinned by
  `tests/watchpoints.rs::press_stops_carry_buttons_and_port_and_run_frames_does_not`.

- **`af434a2` — §11.8, CR-11 and CR-12.** Four new method fragments
  (`emulator/watchpoint_add`/`_clear`/`_list`/`_hits`), `$defs/watchStamp`, `capabilities.watchpoints`, the
  `watch` param on `emulator/stopped` (this one **does** have a discriminator, so both directions are
  enforced by `if`/`then`/`else`), the `censusKey`-without-`mode:"census"` refusal as a two-way `if`/`then`,
  §5's `-32005 watchCapReached` reason, and a new **§8 item 21**.

  Three rules in these fragments are structural rather than stylistic, and each is pinned by a test here:
  a hit's **`old` is present iff `space != "bus"`** and **`fc` iff `space == "bus"`** (an `if`/`then`/`else`
  inside `hits[].items`); the watch **handle is a string at all five places it appears**; and both list
  results take §2.4's **flat** bounded-list spelling — `total`/`returned`/`limit`/`truncated` as siblings of
  the array, not a nested `boundedList`.

### What the earlier re-vendor adopted

One contract commit, **`f309cc8`** — the result-key ruling (`protocol.md` §11.5), which nearly doubled the
schema (30,075 → 59,356 bytes). Four things landed, and three of them change our wire:

- **12 new result fragments.** Every advertised method now has one, so `tests/schema_conformance.rs`'s
  `UNCOVERED_METHODS` goes from 12 entries to **none**. This is the direction that counts: the fragments
  were written upstream from the ruling, not derived here from what we emit.
- **§4 rewritten, and `lookup_symbol` changed on three counts.** `name` is the identifying spelling on
  every branch and MUST round-trip — `$defs/symbolName` rejects a `+$hex` displacement suffix by pattern,
  which is what our address direction used to emit. `rawName` is **struck**. `exact` becomes REQUIRED and
  present on both name-direction branches. `otherMatches` becomes `$defs/boundedList` with one pinned item
  shape and **no `cursor`, no `nextCursor`**.
- **§2.4, new: the shared result conventions** — `caveat` specified once for the whole bus, and the
  bounded-list rule (a)–(d). Clause (b) is why `rpc::bounded_array` stopped emitting a continuation token:
  a method that accepts no cursor param must not emit one.
- **§8 item 20, new:** a server's conformance suite MUST close every result against its fragment, as
  `unevaluatedProperties: false` applied **at test time** and deliberately not published. Implemented in
  `common::schema::closed`.

**CR-14's registered divergence is retired in this commit** — the mechanism working as designed: the ruling
landed upstream, the copy was refreshed, and `every_registered_divergence_is_still_live` failed on the next
run because the shape it registered was no longer rejected. That failure is the reason the entry is deleted
rather than quietly wrong.

**And CR-16 was raised by this re-vendor**, on the first run with item 20's closure live: five keys that
§11.5's own prose registers by name — `initialize.limits`, `initialize.methodSummaries`,
`read_memory.region`, `read_memory.symbolDisp`, `read_memory.caveat` — never reached their fragments. Two
fragments out of 22 were left behind by a large amendment. Registered, not silenced; see
`docs/2026-08-14-aether-change-requests.md`.

## Re-vendoring

> ### ⚠ While `TRACKED_REVISION` is `Some`, copy from the OBJECT STORE, never from the checkout
>
> The sibling `empyrean/` working tree is on whatever branch someone last checked out — normally the
> default branch, which while a draft is being tracked holds the **pre-amendment** schema. A `cp` from it
> therefore *downgrades* the vendored copy, and the downgrade is quiet: the freshness test's plain compare
> sees vendored == upstream working tree and returns early, so **it goes green on the wrong file**. What
> actually goes red is some unrelated suite, whose obvious "fix" is to change the server to match a schema
> that has silently gone backwards. Take the bytes from the revision by name instead.

When the freshness test goes red, while a draft revision is tracked (`TRACKED_REVISION` is `Some` in
`tests/schema_conformance.rs`, and the ⚠ box near the top of this file is present):

```sh
REV=<the contract revision this copy should track>
git -C /home/volence/sonic_hacks/empyrean show "$REV:contract/schema/bus-protocol.schema.json" \
   > crates/oracle-aether/tests/contract/bus-protocol.schema.json
sha256sum crates/oracle-aether/tests/contract/bus-protocol.schema.json
# The revision the copy tracks, and the last commit that actually moved the schema — record BOTH, they
# differ whenever a prose-only ruling round lands on top of a schema change.
git -C /home/volence/sonic_hacks/empyrean log -1 --format='%H %s' "$REV"
git -C /home/volence/sonic_hacks/empyrean log -1 --format='%H %s' "$REV" -- contract/schema/bus-protocol.schema.json
```

Once the draft has merged and `TRACKED_REVISION` is back to `None`, the freshness test compares against
the checkout again — but **copy from the object store even then**, and record the blob:

```sh
git -C /home/volence/sonic_hacks/empyrean fetch -q origin
git -C /home/volence/sonic_hacks/empyrean show origin/main:contract/schema/bus-protocol.schema.json \
   > crates/oracle-aether/tests/contract/bus-protocol.schema.json
# Re-verify what you WROTE, not what you read, and record all three coordinates.
sha256sum crates/oracle-aether/tests/contract/bus-protocol.schema.json
git hash-object crates/oracle-aether/tests/contract/bus-protocol.schema.json
git -C /home/volence/sonic_hacks/empyrean log -1 --format='%H %s' origin/main \
   -- contract/schema/bus-protocol.schema.json
```

> **`empyrean/` is a live working tree, not an archive.** An earlier version of this section said that
> with `TRACKED_REVISION` at `None` "the checkout *is* the authority and the plain copy is correct
> again". That is true of the *freshness comparison* and false of the *copy*: another session edits that
> tree, so `cp` means "whatever is on disk at this instant", and the instant is not recorded anywhere. On
> 2026-08-22 the two happened to agree and the `cp` would have been harmless — which is exactly how the
> habit survives to the day they disagree. Take the bytes by name; the extra keystrokes buy an auditable
> pointer.

The same hazard has a second face inside a re-vendor, and it drew blood on 2026-08-22: a red-first script
backed the vendored file up with `cp` before doctoring it, and an earlier aborted run had already left
that file doctored, so the "pristine" backup was the corrupted copy. The restore reported success and the
sha did not match. **Restore from the object store by blob id, and check the sha of the restored file**
— a backup taken from a file you have been mutating is not a baseline.

Either way: update the table above with the new commit and hash, then run `cargo test -p oracle-aether`. If
the new schema rejects messages the server sends, **that is the point** — contract §8 item 15: where a
server's shape and the schema disagree, the server changes. Never the wire silently.

## Locating the upstream copy

The freshness test looks for the sibling checkout, in order:

1. `$AETHER_CONTRACT_SCHEMA` — an explicit path to the upstream schema file.
2. Ancestor directories of `CARGO_MANIFEST_DIR`, each probed for
   `empyrean/contract/schema/bus-protocol.schema.json` (this finds it from a normal checkout *and* from a
   `.claude/worktrees/…` worktree, whose depth differs).

If none hit, the test **fails loudly** rather than passing — see the comment on
`the_vendored_schema_is_byte_identical_to_the_upstream_contract` for why, and for the
`AETHER_CONTRACT_OPTIONAL=1` escape hatch.

### CR-16, adopted hours after `f309cc8` and retired the same day

`d45dc87` adds five `properties` entries across two fragments — `initialize.limits`,
`initialize.methodSummaries`, `read_memory.region`/`.symbolDisp`/`.caveat` — all of which `protocol.md`
already **registered in prose** and none of which reached the schema. `limits` joins `initialize`'s
`required`, `region` joins `read_memory`'s. No prose changed; the prose was already right.

It was found by §8 item 20's closure on its first run, in the document rather than in the server. Its
registry entries and their key-checkers are gone from `tests/common/schema.rs`, and that retirement was
**forced, not remembered**: those checkers *lift* their key out of the payload before validating it, so the
moment the schema required `limits`, lifting it made it missing — and every checkpoint test went red on the
handshake. An allowance that outlives its divergence does not go stale quietly; it starts causing the
failure it was written to suppress, in tests unrelated to it.

One fixture moved with it: `schema_conformance.rs`'s `good_read_memory_reply()` omitted `region` and so
stopped being conformant the moment the fragment declared it — the positive control catching its own drift,
which is the only reason the rejection controls beneath it stayed meaningful.

### `432f631` — a description fix, no shapes touched

The schema's `title` and `description` still called it a **SEED** with *"a representative set of ops"*,
written when 9 of 21 advertised methods had a `result`. It is now 23 of §6's ~60 catalogued methods, which
is **every method the reference server advertises**, both halves. The old wording understated the artifact
at its front door — the first thing a new consumer reads — so it now states exactly what is and is not
covered, and points at §8 item 20 as the reason an unschematized method cannot quietly ship a result.

**No shape changed**, so this re-vendor cannot move a single validation verdict; the freshness test still
demands it, which is the point of byte-identity.

### `34a1993` — CR-17, the amendment the previous amendment made necessary

`minimum: 0` on `run_frames.frames` and `press.frames`. §11.8's `stopAfter` made a bounded advance able to
end inside its own first frame, where the truthful whole-frame count is **0** — and the field that counts
frames still had a floor of 1, leaving a conformant server no legal way to say what happened. The server
had shipped a round-to-1 with the reason at the site and raised it rather than absorbing it; the rounding
is now gone. `stopped.frames` is deliberately unchanged at `minimum: 1` — see §11.9 for why the reply and
the event are not the same field with two homes.
