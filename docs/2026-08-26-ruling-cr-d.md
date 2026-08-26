# Adjudication of CR-D — the object decoders: a closed envelope over an engine-shaped payload (2026-08-26, independent adjudicator)

Applies to `docs/2026-08-26-cr-d-object-decoders.md` at oracle `main` merge
`24f400ec4981d18638cedfb33dde5facb928e350` (file blob `2134864`, commits
`1a7ae2b7`..`9cc293f9`; the blob is unchanged at the worktree base `8709790`), and the contract it
targets — empyrean `contract/protocol.md` and `contract/schema/bus-protocol.schema.json` — read at the
CR's anchor `39cfaa27c293510d583581b5b07d07709691508a` and re-checked at `origin/main` after `git fetch`
at ruling time, `78d432235090ae53848f4f6725f36ac148ff1ef4`. **The tip moved during the CR's life
(`39cfaa27` → `78d4322`, seven commits) but neither contract file did**: `protocol.md` is blob
`b4776ce90000a89ee50755892f999c03e5130e99` and the schema blob
`7b24bcedc24f0a6aa7dd4504f4e2f9bf63e4cda7` at **both** revisions (the one commit touching `contract/`
in between, `209c7fe`, edits `contract/projects.json` only). So unlike CR-C, this ruling adjudicates
against exactly the artifact the CR read. The engine was read at aeon
`f48961396848de666a737971bbb7b1c627a90f78` (tree revision; a docs commit, ancestor of
`origin/master`, both verified), the sweep tree at `b87e6e5a`, the legacy server at oracle-old
`9dc67c5b`, the successor at oracle `0d7c5c21`, the consumer-against at aurora `e5dd32a5`, and
`sonic_hack` at `858af72c` — every sibling file through `git show <rev>:<path>` or `git grep <rev>`,
never a working-tree path. No emulator MCP tool was used; no `cargo` was run; no server was started;
nothing was committed to any `main`; `docs/lane-status.json` was not touched. Everything executed was
`git`, `grep`, `sed`, `awk` and short Python written for this ruling. The ruling model is **Claude
Fable 5**, and it was **un-framed**: it received the CR, the contract, the precedent ruling's *form*,
and no steer on the outcome.

## Ruling: **ADOPT WITH CHANGES** — D1, D2, D4 as drafted (with the M-fixes below); D3 adopted (Q1 answered *serve*); D5 adopted (Q8 answered *travel*)

The evidence base is sound to the line, and to a degree that has earned the CR deference on the
arguments built from it: **every citation I resolved reproduced** — every protocol.md and schema
anchor, every aeon source line including the full SST offset table, all ten overlay declarations at
the exact files and line numbers claimed, every legacy C++ line, every consumer-sweep count in all
seven repos (A/B/C, files, slash-form — reproduced by re-running the CR's own enumeration commands),
and both exhibits of the fixture-vs-demand address discrepancy. The corrected §3.2/§7.1 replacement
fact was **re-derived here from scratch**, not accepted: the ten-overlay enumeration reproduces
exactly, the five-way `$30` table reproduces field-for-field, the byte-budget forward sum (18 through
`even_pad`, 26 total) balances against the source's own stated budget, and the refuting sentence
really does sit inside the quote the original claim leaned on. One arithmetic slip was found *inside
the correction itself* (S1), and it does not move the conclusion.

The central design question — may two of the eight deliberately-unschematized rows carry fragments —
is answered **yes, by this mechanism**: the typed-open `fields` map with a REQUIRED `layout`
discriminant is the audit's own unblock condition for D-27 (verified verbatim), it has shipping
precedent in `methodSummaries` (verified: schema line ~610, *"its key set MUST equal `methods`"* —
schema pins the value type, prose pins key provenance), and it does not depend on the harness's
closure depth (Q6, pinned below). The resolution is not a loophole: the openness is a declared,
subschema-carried property, which is precisely what the `$comment`'s "partial fragment" objection is
*not* about — a partial fragment lies about completeness; a typed-open map states its incompleteness
as a type. But the CR's deltas as drafted contain one incompleteness and one structural bug that
would each ship a contradiction (M1, M2), and two of its handed-over questions need more precise
answers than it sketched (M3, M4). Hence adopt-with-changes.

---

## Every checkable claim, checked

Legend: **HELD** = reproduced firsthand at the named revision; **ADJUSTED** = substance holds, detail
corrected; **FAILED** = the tree or the arithmetic says otherwise; ⟨RUNTIME⟩ = would need a running
process.

### Contract (empyrean `39cfaa27` = `78d4322` for both files)

| # | Claim | Result |
|---|---|---|
| 1 | Blobs `b4776ce9` (protocol.md, 4265 lines) / `7b24bced` (schema) | HELD, both revisions |
| 2 | §6 group `### object / player decoders ⚙` at `:1492`; rows `:1496-1497` verbatim (`objects[]{slot,…,x,y,class}`, *"engine-dependent decoded player struct(s)"*); ⚙ note `:1500-1503` with the SHOULD-surface sentence | HELD (the note spans to `:1503`, CR says 1500-1502 — immaterial) |
| 3 | §8 item 20 at `:2142`, *"fail on any result key absent from that method's fragment"*, `unevaluatedProperties: false` at test time, deliberately NOT published | HELD |
| 4 | §9 Phase-5 deferral at `:2277-2278`, wording as quoted in §10.2 | HELD |
| 5 | Last amendment §11.24 at `:4227`; file ends `:4265`; next free is §11.25 | HELD |
| 6 | Schema `$.methods` has **60 keys = 59 fragments + `$comment`**; neither decoder present; all eight named rows absent | HELD by parse |
| 7 | Schema line-5 `$comment`: *"EIGHT §6 ROWS REMAIN UNSCHEMATIZED AND DELIBERATELY SO …"* with the partial-fragment sentence quoted in §1 | HELD verbatim — **and it also says the gate "prints all eight on every run", and §10.5 never amends it: M1** |
| 8 | The eight-row set derived from §6 table rows matches the `$comment`'s list, reverse difference empty | HELD (re-derived by my own parse + regex over `:896-1927`) |
| 9 | `methodSummaries` typed-open (`additionalProperties: {"type":"string"}`) with the prose key-provenance clause | HELD |
| 10 | `capabilities` an open object; `objectDecoders` `{"type":"boolean"}`; `checkpoints`/`watchpoints` are `{supported,…}` objects | HELD by parse |
| 11 | `limits` declares ten keys, requires three (`maxRunFrames`,`maxReadLen`,`maxLineBytes`) | HELD by parse |
| 12 | D9 four categories at `:113-130` as characterized | HELD |
| 13 | §4: *"a mismatched listing is not degraded information, it is confidently wrong information…"* (`:845-847`); *"`name` is the identifying spelling, and it MUST round-trip"* (`:776`); *"A displacement is never inside a name string"* (`:782`) | HELD verbatim |
| 14 | §2.4: flat bounded-list spelling (`:621-630`); clause (b) no-cursor; clause (d) structural→neither; caveat declaration rule + *"a caveat every reply carries is one nobody reads"* (`:559-563`, `:1016`) | HELD |
| 15 | §2.5: `-32602` + `error.data.unknownParams` + *"The refusal precedes any effect"* (`:663-666`) | HELD |
| 16 | Error registry: `-32012` *no symbols loaded* (`:870`), `-32004` *address out of range / write rejected* (`:867`) | HELD |
| 17 | `write_memory` *"strict by design: relaxing a refusal later is additive (D5); introducing one is not"* (`:1052`); `bytes`-XOR-`value`+`width` (`:1464`) | HELD |
| 18 | §11.18: widening an emitted enum is not additive (`:1852-1854`) | HELD |
| 19 | §11.5's `additionalProperties`-blind-past-`allOf` (`:2677-2679`); `otherMatches.items[]` self-closure | HELD |
| 20 | §11.21 *"Legacy is frozen, not migrated"* (`:4039`); §11.23 exists as CR-C's landing (`:4088`) | HELD |
| 21 | `emulator/sprites` fragment: flat-spelling `$comment`, `total` `{"const":80}`, per-entry `satAddr` struck, *"derivable key CR-13 struck"* | HELD verbatim |
| 22 | `get_profiler_frames`: *"rows are keyed by entry address, never by symbol"* | HELD verbatim |
| 23 | Audit D-27 (`audit.md:340-351`): full text incl. the unblock sentence §1 quotes | HELD verbatim |
| 24 | Audit §3 preamble: the *"actively refuses … while looking complete"* passage (`audit.md:307-311`) | HELD |
| 25 | `ASSEMBLER_VISION.md` plans `debug/struct_layout` (item 3 of the debug-info list, ~`:154`) | HELD |
| 26 | camelCase normative "for fields" citing `:712-713`, `:2332` | **ADJUSTED** — both anchors exist and say what they say, but they are the *event-name* rule (§3) and its §10 resolution; field-key camelCase is the contract's practice (`protocolVersion`, `frameToken`) rather than a rule those two lines state. Substance (don't inherit the legacy's snake_case keys) unaffected |
| 27 | Gate G5 *prints* the unschematized rows | HELD — **and the gate DERIVES the list by diffing §6 against `methods` (`validate_contract_schema.py:34-36`, `:217-223`), so the gate self-corrects on adoption; only the schema `$comment` hardcodes the eight (M1 narrows to the `$comment` + vectors)** |
| 28 | §11.17 clause 7: fragment count re-derived by parsing; red-first vector practice (`:3705`, `:3887`, `:4050-4051`, `:4195`) | HELD — the CR's §10 proposes fragments with no vectors and no recount; folded into M1 |

### aeon (`f4896139` for sources; `b87e6e5a` for the sweep)

| # | Claim | Result |
|---|---|---|
| 29 | `f4896139` is a docs commit (+1 `docs/lane-log.jsonl`), ancestor of `origin/master` | HELD |
| 30 | §2.4's full SST table — every field, offset, type and note; `(size: $50)`; offsets pinned in-file (*"a drifted field fails the module's own lowering"*) | HELD, field-for-field against `sst.emp` |
| 31 | `sst_custom: [u8; 32] @ $30` with the *"overlay window (`vars X: sst_custom`)"* comment; `SST_interact` = `sizeof(Sst) - 2`, *"NOT a Sst struct field"*, 30 game-usable bytes | HELD verbatim |
| 32 | The sst-fold comment dating `$52 → $50` to 2026-08-05; `core.emp:52` still reads *"Since the SST grew to `$52`"* | HELD, both |
| 33 | `objcodebase.emp:4-6`: bank at `$10000`, `code_addr = label - ObjCodeBase`, `objroutine(0)` = the safety `rts` | HELD |
| 34 | Pool-detection algorithm at `core.emp:207-215` (inside `DeleteObject`'s header), adjacency `ensure` at `:35` | HELD |
| 35 | `ram.emp:612-618` layout block verbatim; `constants.emp:78-90` — `NUM_PLAYERS=2, NUM_DYNAMIC=40, NUM_SYSTEM=8, NUM_EFFECTS=16`, total 66 | HELD |
| 36 | The demand's table stops at `frame_off @ $2E` — no `sst_custom`, no `SST_interact` | HELD (read from the demand doc directly) |
| 37 | Fixture `tools/fixtures/s4_listing_excerpt.lst` gives `Dynamic_Slots : FFFF8DC2`; demand gives `Player_1` `$FF8DB0`; `$FF8DB0 + 2×$50 = $FF8E50 ≠ $FF8DC2` | HELD — arithmetic re-done here; the two artifacts really do describe different builds |
| 38 | **§2.4.1: ten `vars … : Sst.sst_custom` declarations, nine files, none targeting any other window** — the CR's exact regex re-run over `engine/**` `games/**` | HELD — ten hits, identical files **and line numbers** (`dust_spindash.emp:140`, `path_swap.emp:49`, `test_animated.emp:19`, `test_churn.emp:33`, `test_enemy.emp:19`, `test_helpers.emp:16`, `test_parent.emp:33`, `:42`, `test_player.emp:31`, `player_common.emp:89`) |
| 39 | The five-way `$30` table: `PlayerV.ground_speed: i16`, `DustV.player: u16` (an SST pointer), `TEnemyV.steps_remaining: u16`, `EmitterV.timer: u16`, `PathSwapV.half_height: u16` | HELD — all five first fields read from source |
| 40 | §12.5b's split: seven overlays in `test_*.emp`, three release (`PlayerV`, `DustV`, `PathSwapV`); PathSwap art *"TEST placeholders only"* but the object a real invisible trigger (`col=COLLISION_NONE`, *"never rendered in release"*) | HELD |
| 41 | **§3.2's correction, re-derived**: the refuting sentence (*"The language cannot express the union as byte-SHARING (every overlay starts at `$30`), so the fields are declared in place and this reads as a budget principle"*) sits in `player_common.emp` exactly where claimed; the fields `fly_fuel`/`fly_thrust`/`glide_angle`/`knux_step` are sequential distinct offsets; the forward width-sum is 18 through `even_pad` and **26** total, matching the source's *"the window has 30 game-usable bytes and PlayerV now spends 26"* — so the original union-as-bytes claim was wrong and the correction right | HELD — **with one slip inside the correction: the counterfactual "sharing would give 22" does not reproduce (S1)** |
| 42 | §7.1(a) liveness: *"Player_Init deliberately does NOT clear"* the ability scratch; writers reachable only through `CharDef_Tails.cd_ability`/`PSTATE_FLY` (and the Knuckles analogue); *"the whole point is that Sonic's frame never touches these addresses"* | HELD verbatim |
| 43 | Sweep, aeon: A=45 lines / 22 files, B=11, C=0; source split 20 (all `player_state`, 0 `object_list` in `engine/**`+`games/**`) | HELD — re-run, exact |
| 44 | Exhibit lines `docs/BUGS.md:1119` and `docs/benchmarks/effects-p2/GATE-EVIDENCE.md:119` (*"address from `emulator_player_state`, never from a doc"*) | HELD verbatim |
| 45 | The name collision: `player_common.emp:91` `player_state: u8` (PSTATE_* byte at `sst_custom+2`) | HELD |

### Legacy server (oracle-old `9dc67c5b`) and `sonic_hack` (`858af72c`)

| # | Claim | Result |
|---|---|---|
| 46 | Dispatch registrations `{"object_list", OpObjectList}` / `{"player_state", OpPlayerState}` (~`:2656-2658`); noisy-op list (`:2834-2836`); MCP shim tool entries (`oracle_mcp.py:701`, `:709`) | HELD |
| 47 | `OpObjectList`: no params; one top-level key `objects`; skip on `codeAddr == 0`; 5 item keys `{slot,pool,x,y,class}` / `{slot,id,x,y,class}`; `class` emitted as `""` when empty | HELD (function spans `:1088-1145`, CR says `-1142` — immaterial) |
| 48 | `OpPlayerState`: top level `{ok,engine,player_1,player_2}` on the aeon branch vs `{ok,main,sidekick}` on the other — `engine` on one branch only; active item = 12 keys, inactive = 2 (`{active:false, addr}`); `stBits` hardcoded at `:1187-1190` = `b0, xflip, yflip, in_air, rolling, on_object, pushing, underwater`; the other branch spells `air`/`onobject`; `status2` placeholders `s2b0…s2b4` | HELD, all |
| 49 | `DetectSST` branches on `Player_1` (`:935-941`); `S4PoolName` hardcodes 2/40/8 (`:944-950`); `S4ClassName` strips `_Main`, `ObjCodeBase` fallback `0x10000`, `romAddr = (codeBase & 0xFFFF0000) | codeAddr`, nearest within `$100` (`:951-964`) | HELD |
| 50 | `0xFFB000` fallback at `:985` (`object_slot`) and `:1097` (`object_list`); stride `slot * 0x50` computed before the engine branch (`:987`, `:1103`) | HELD |
| 51 | Slot-out-of-range answers `-32004`: message *"slot out of range (0-%d)"* → `CodeForMessage` maps `"out of"`/`"range"` → `-32004` (`:217-224`) | HELD (via the message-classifier, as the CR's mechanism implies) |
| 52 | `object_slot` omits `class` when empty (`if (!cls.empty())`) — one datum, two absence conventions across the family | HELD |
| 53 | §2.3's retracted stride hypothesis: `sonic_hack` `object_size = $50` (`S4.constants.asm:172`); 12 reserved slots + `$60`(=96) dynamic = 108 = the legacy's `maxSlots` | HELD — recounted the `ds.b object_size` lines myself |

### Successor, consumers, sweep totals

| # | Claim | Result |
|---|---|---|
| 54 | `engine.rs` at `0d7c5c2`: `objectDecoders: false` at `:1393`; `romLoaded` at `:1420`; `fn symbol_at` at `:1559`; **44** unique `"emulator/…"` literals = 41 methods + 3 events; `lookup_symbol`/`load_symbols` present; neither decoder present | HELD, all |
| 55 | `romLoaded` appears nowhere in either contract artifact (§12.8) | HELD (0 hits in both) |
| 56 | Vendored schema at `0d7c5c2` hashes to `7b24bced…` = upstream blob | HELD (`git hash-object`) |
| 57 | `tests/common/schema.rs::closed` doc comment: closure at the top level of the result, `otherMatches.items[]` the sanctioned nested case | HELD verbatim (comment at ~`:126-129`, fn at `:130`) |
| 58 | Peer answers §6: *"the biggest shape hazard in the eight"* (`:909-910`) | HELD |
| 59 | `docs/OVERSEER.md`'s acceptance list = 21 schematized-and-unserved methods, decoder rows not among them | HELD — the enumerated list contains no decoder row |
| 60 | aurora's §12.1 argument quoted in full from `e5dd32a:docs/reviews/2026-08-22-oracle-instrument-gaps.md:307-315` under heading 2.6 at `:305` | HELD verbatim |
| 61 | Sweep totals: aurora A=1, seraph A=0, sigil A=10, oracle-old A=14, empyrean A=18/B=2/slash=2, oracle A=24 (13 files)/B=4/slash=4 | HELD — every row re-run |
| 62 | Q2's fulcrum, checked beyond the CR: `sprites.satBase` is `$defs/hex` with `"D9 category 1"` in its own description **while** its `$comment` invites the client to compute `satBase + index*8` | HELD — the shipping precedent already separates representation (cat 1 hex) from permitted computation (cat 2), which settles Q2 and S5 |

**Tally: 62 rows; 60 HELD, 2 ADJUSTED (26, and the S1 slip inside row 41), 0 FAILED.** Every anchor I
attempted resolved; none was unreachable. That is a better record than CR-C's, and it is why the
design arguments below are judged on their merits rather than discounted.

---

## §13.1 — the eight questions, answered

**Q1 — Does the successor owe the legacy's decoder surface at cutover? The premise of reversal
condition (a) is TRUE and the reversal is nonetheless DECLINED: serve D3.** The documentary fact
verified (row 59): `OVERSEER.md`'s acceptance contract is built from the schematized-and-unserved set
and cannot contain these rows. But that instrument is the wrong one for "owed" — its membership is
defined by schematizability, so its silence about unschematizable rows is circular, and the CR itself
says "by construction". What decides "owed" is the consumer record, and that record is unusually
concrete: a demand *filed by the consumer* naming **both** methods in its title and measuring both
`no such method` against the successor; eleven written reliance records in aeon including one
instructing itself to prefer `emulator_player_state` over its own docs (row 44); and — the fact that
makes the loss current rather than prospective — the MCP shim now spawns the successor by default
(§11.23's landing, verified in the CR-C ruling), so the tools those sessions name are *already*
broken, which is precisely what the demand measured. On condition (b): the derivability objection is
weaker than §7.5 grants it, because what is derivable from `object_list` + `pools[0]` is the *facts*,
not the *answer-form* — `active: false` as a stated fact and `role` are not in any `object_list`
reply, and §9.2's own argument (a client should not infer "player 2 is absent" from an array's
length against a bound it must join from elsewhere) applies. Finally the family-coherence point is
decisive now that M1 forces the schema `$comment` to be rewritten anyway: leaving `player_state`
unschematized for a reason ("derivable") the set's own stated reason does not cover would make the
remaining set lie about itself. **D3 is adopted, with M2's fragment fix.** The margin is the CR's
"reduced confidence" margin, honestly reported; an owner who later strikes D3 pre-ship breaks nothing.

**Q2 — `code` is a hex string (D9 category 1), as drafted.** Settled by a precedent the CR did not
cite (row 62): `sprites.satBase` is `$defs/hex`, labels itself category 1, and *in the same
description* licenses the client to compute `satBase + index*8` under category 2. The bus already
distinguishes a value's representation (address-shaped → hex string) from the arithmetic a client may
do after parsing it. `code` is address-shaped (a routine offset; the legacy's other branch reads a
byte id — width varies by layout, which no JSON number communicates), it travels beside `addr`,
`baseAddr` and `bytes` which are all `$defs/hex`, and it is an identity, not a quantity. §6.3's
appeal to "category 2 permits the client to compute" stays true under this ruling — reworded per S5
so it reads as satBase's pattern rather than as a category claim about `code` itself.

**Q3 — Keep `includeBytes`.** Three grounds. (i) It is the demand's own first spelling of its ask
(*"the raw `sst` bytes, or at least…"* — verified). (ii) `emulator/read` cannot substitute in one
call: the pool is 66 × `$50` = 5,280 bytes and the catalogued `read` cap is 4,096 (`:989`), so
"capture the whole pool" is at minimum two reads with no per-slot slicing and no `layout` stamp tying
the bytes to the decode. (iii) It is off by default, so the default reply's key set stays fully
enumerated. The atomicity argument — bytes and decode arriving under one envelope stamp and one
`layout` — is real on a free-running machine and is what `read`-then-decode cannot give.

**Q4 — Yes: refuse `-32012` when no symbols are loaded; do not inherit the `0xFFB000` fallback.**
The fallback is P2's confidently-wrong shape with no `binding` field (row 50 shows it live at two
call sites), `write_memory`'s strict-by-design precedent is verified (`:1052`), the error code's
registered meaning fits exactly (`:870`), and the escape hatch is genuine: a server that wants a
configured base answers with `detectedBy: "configured"` and a caveat, losing nothing. Relaxing later
is additive; un-relaxing is not.

**Q5 — `-32602`, with `error.data` carrying the bound (e.g. `{"slot": …, "slotCount": …}`) — but the
CR's framing missed that the contract itself is split.** The CR presented Q5 as CR-vs-legacy. In
fact the contract carries two live precedents that disagree with each other: `pixel_attribution`
refuses a dot outside the (runtime-sized!) active display with **`-32004`** carrying
`width`/`height` in `error.data` (`:1294`, and `:1373` says in terms no static schema can bound it —
structurally identical to a slot index bounded by the loaded game), while `scanlines` refuses
out-of-range rows with **`-32602`** (§11.14, `:3220`). Ruling: follow the newer precedent —
`-32602`, because a slot index is a parameter and §2.5 has made `-32602` the params-refusal code with
typed `error.data`; and §11.25 must *name* the `pixel_attribution` tension rather than present
`-32602` as unopposed (S2). Reachable only with D5, which is adopted (Q8).

**Q6 — Pinned: item 20's closure applies at the top level of the result object; nested objects are
closed only where the published subschema closes them itself.** Grounds: (i) it is item 20's literal
subject ("any result key"); (ii) it is the reference harness's shipping reading (row 57); (iii) —
and this corrects the CR's own framing that the scope is *"settled only by a code comment in one
implementation's test harness"* — the contract already half-states it: §2.5's normative text reads
*"The closure is at the top level of `params` — **item 20's own scope**, for its reason"*
(`:666-668`), which is contract prose asserting the top-level reading of item 20. The pin therefore
confirms rather than chooses. M4 lands the sentence in item 20's own text so the rule stops living
in a cross-reference and a test comment. Under this pin the CR's fragments are legal twice over, as
its §3.3 point 3 intended.

**Q7 — (a), but only with the precision in M3, because (a) as worded does not work.** A use-site
`additionalProperties: false` beside `allOf: [$ref decodedSlot]` is blind past the `allOf` **in both
directions** — it would refuse `slot`, `addr`, `x`, `y`, `code` themselves. The working form of (a):
`decodedSlot` is factored **unclosed** (types + `required` only); every use site re-lists **all**
permitted key *names* in its own `properties` (inherited keys as `true` schemas, its additions
typed) and closes with `additionalProperties: false`. That is (a) for the shapes and (b) for the
names — the DRY benefit the CR hoped for from (a) partly evaporates, and the text should say so
rather than let an implementer ship the broken middle form. See also M2, which changes what the
player item must express at all.

**Q8 — Yes, D5 travels.** The audit itself records that `object_slot`'s param was withheld *only*
under the no-half-fragment rule (row 23); the marginal cost after D2 is one params object and a
hoisted item; and the set-coherence argument that decided Q1's third leg applies with more force
here — once the `$comment` is rewritten (M1), a decoder family split two-schematized/one-not would
need a reason the remaining set's stated reason does not give. The alias-under-`object_list.slot`
alternative is correctly declined for the reason the CR gives. Q5's `-32602` applies.

---

## §13.2 — the twelve settlings, each given a position

1. **`fields` map beats an enumerated field list — STANDS**, on evidence this ruling reproduced
   independently: the ten-overlay enumeration is exact to file and line (row 38), the five-way `$30`
   table is exact to field (row 39), and even discounted to the three release overlays (row 40) the
   argument holds — a signed inertia, an SST pointer and a pixel extent at one offset in one build is
   the whole case. The settling's own instruction to aim objections at §2.4.1 is fair, and §2.4.1
   survives the aim.
2. **`layout` REQUIRED and per-reply — STANDS.** P2's textual basis verified (row 13); the
   detect-after-handshake argument verified in the live mechanism (`DetectSST` on `Player_1`,
   `load_symbols` callable any time); the silent-`0xFFB000` hazard verified at two call sites
   (row 50); the fixture-vs-demand address discrepancy verified by my own arithmetic (row 37). This
   is the CR's best-argued key and the one REQUIRED spend of the pre-release window that is clearly
   right.
3. **`objectDecoders` stays a boolean — STANDS.** Changing a published key's JSON type is not
   additive; `checkpoints`/`watchpoints` were *born* objects (row 10), so they are precedent for new
   keys, not for retyping. With S4's precision on what `true` means for a partial family.
4. **No decoded bit-name enums — STANDS.** The invented names and the cross-branch disagreement are
   verified in the C source (`in_air` vs `air`, `on_object` vs `onobject`, `b0`, `s2b0…` — row 48);
   §11.18's non-widenability verified (row 18). The observation that `bits` carries strictly less
   than `raw` (a set-bits list cannot express a clear bit) is correct and worth keeping in §11.25.
5. **`players` is an array — STANDS.** The branch-varying key set is verified in source (row 48) and
   this repo's transcription already called it the family's biggest hazard (row 58).
6. **`name` bare and round-trippable; absence an omitted key — STANDS.** §4 verified (row 13); both
   legacy violations verified (`_Main` strip row 49; `""`-vs-omitted rows 47/52).
7. **No `cursor` — STANDS** (§2.4 clause (b), row 14; `lookup_symbol` precedent).
8. **Pure reads — STANDS** (the `read`/`sprites`/`pixel_attribution`/`scanlines` family, `:1011-1016`).
9. **`caveat` declared, emitted conditionally — STANDS** (row 14).
10. **No new §8 item — STANDS, with a reservation the CR should record**: its own criterion — CR-A
    and CR-C *"earned theirs by creating an obligation no fragment could express"* — cuts against
    it, since §10.6 lists **five** such obligations. The settling survives because those five are
    per-engine and not mechanically checkable by a generic harness, so an item would add wordage
    without verifiability; but §11.25 should say that, not leave the tension unspoken.
11. **No rename — STANDS.** The collision is real (row 45) and verified, and a live MCP tool name is
    the right thing to spend it on.
12. **The legacy server is asked for nothing — STANDS** (§11.21's frozen-not-migrated verified,
    row 20; §2.7's five findings correctly framed as a work list only if it is ever pointed at these
    fragments).

---

## MUST (blocks adoption as drafted; each names its defect and its fix)

**M1 — §10's deltas are incomplete: the schema's own line-5 `$comment` still names the adopted rows
as unschematized, and no vectors are proposed.** The `$comment` (row 7) hardcodes the eight rows by
name, says "EIGHT", and asserts *"the gate … prints all eight on every run so none goes quiet."*
§10.5 adds fragments for two or three of them and never touches it. Adopting as drafted ships a
published artifact that simultaneously carries `emulator/object_list`'s fragment and a sentence
declaring it unschematized — a D14-class live contradiction sitting in the very text the CR's §1
quotes as its spine, and the exact "count carried in prose" defect the same `$comment`'s other half
memorializes. (Mitigation found here: the gate *derives* its G5 list by diffing §6 against
`methods` (row 27), so the script self-corrects; only the `$comment` lies.) Fix, three parts:
(i) §10.5 gains a delta rewriting the `$comment` — the remaining set is **five** (`call_stack`,
`log_tail`, `z80_registers`, `read_vdp_registers`, `read_vsram`), the stated reason still true of
all five, with a pointer to §11.25 for how the decoder three left; (ii) the landing follows §11.17
clause 7 — fragment count re-derived by parsing the merged `methods` object, never carried over;
(iii) pass- and red-vectors for the new fragments per the CR-BP M2 practice (`:4050-4051`), the
refusals proven red-first.

**M2 — `player_state`'s item as sketched refuses the very reply §7.2 mandates.** §7.2: *"When
`active` is `false`, `slot` and `addr` are still present … and the rest are omitted."* §10.5:
the item is `decodedSlot` (whose `required` is `["slot","addr","x","y","code"]`) composed with
`active`/`role`. An inactive player — `{slot, addr, active: false}` — fails `decodedSlot`'s
`required` set: the fragment would reject every reply from a one-player game, which is the
commonest reply the method will ever send. (The legacy's inactive shape, `{active:false, addr}`,
is the live corroboration that inactive items really do omit the core keys.) Fix: the player item
carries its own conditional — `if {active: true}` `then` require the core keys (via the M3
composition), `else` require exactly `slot`, `addr`, `active` and forbid the rest — the same
mechanical `if`/`then` discipline `scanlines` (mode↔width) and the breakpoint family already use.
And `decodedSlot`'s `required` moves to the *use sites* (`object_list` items and `object_slot`
require all five; the player item requires them conditionally), leaving the `$def` as a shape
library, which M3 needs anyway.

**M3 — Q7's mechanism must be stated precisely in §10.5, because both drafted options mislead.**
As ruled at Q7: option (a)'s "restate the closure" must be spelled out as — `decodedSlot` factored
**unclosed**; each of the three use sites re-lists every permitted key name in its own `properties`
(inherited keys as `true`, additions typed) and closes with `additionalProperties: false`; no
`unevaluatedProperties` in the published artifact (item 20's design, reaffirmed at Q6). Without
this sentence the natural implementation of (a) — `allOf` + use-site `additionalProperties` over
only the *new* keys — rejects the base keys, which is §11.5's trap arriving from the third
direction in one CR.

**M4 — Land Q6's pin as contract text.** Add to §8 item 20 one sentence: *"The closure is applied
at the top level of the result object — the literal subject of 'any result key'; objects nested in
a result are closed only where their own published subschema closes them (`otherMatches.items[]`
is the registered case). §2.5 already states the same scope for params."* This is the ruling's
answer to Q6 made durable; it also retroactively legitimizes the reference harness's comment as a
transcription of contract text rather than a private reading, and it is the ground on which the
`fields` map is doubly safe. (The CR's claim that the scope was settled *only* by a code comment
is corrected in the record — §2.5 `:666-668` already asserted it — but a rule this load-bearing
should live in the item that owns it, which is exactly the CR's own point.)

## SHOULD (improves the record; does not block)

**S1 — Fix the correction's counterfactual arithmetic.** §3.2's superseded block and §14 say byte-
sharing *"would give 22"* (a collapse of four). Under the superseded claim's own model — the two
2-byte pairs `fly_fuel`/`fly_thrust` and `glide_angle`/`knux_step` occupying the same bytes — the
saving is **2**, giving **24**; no sharing arrangement of those fields reaches 22. The valid second
derivation is the forward one the CR also states: the declared widths sum to 26, the source's own
budget says 26, and *any* sharing would make the spend less than the sum — so 26 = no sharing. The
conclusion is untouched; the passage claiming two independent derivations should not itself contain
a third number that does not derive.

**S2 — §11.25 must name the `-32004` counter-precedent when recording Q5's answer** (ruled above):
`pixel_attribution` refuses a runtime-structural coordinate with `-32004` + bounds in `error.data`;
this family follows `scanlines`' `-32602` instead, and the divergence is recorded rather than
discovered by the next drafter.

**S3 — Give `fields` values a D9 sentence.** The map pins JSON types (`number|string|boolean`) but
not which convention a given field takes, so two conformant servers could emit `mappings` (a ROM
pointer) as a number and a hex string respectively — a D9 wobble inside the one open surface. One
sentence: *a `fields` value follows D9 — address-shaped fields as `$defs/hex` strings, counts and
scalars as numbers — per the layout's own typing of the field.*

**S4 — Define D4's `true` for a partial family.** "The object-decoder method names appear in
`methods`" is ambiguous when D5 is severed or a server ships two of three. Pin it: `true` iff **at
least one** ⚙ decoder row is in `methods`; per-method presence in `methods` remains the only
per-row warranty (item 23), which the flag never overrides.

**S5 — Reword §6.3's category-2 appeal** to satBase's pattern (row 62): the *representation* of
`code` is category 1; the *computation* `ObjCodeBase + code` is what category 2 permits after
parsing — the same split `sprites.satBase` already ships. Cites the precedent, and Q2 stops being
arguable.

**S6 — Carry §12.5b's discount inline in §2.4.1.** Lead with "three release overlays (ten total,
seven in test objects)" so the number doing the work is the defensible one and the reader does not
need §12.5b to deflate §2.4.1 correctly. The CR already concedes this; the concession belongs where
the number is used.

**S7 — Anchor nits, none material:** `OpObjectList` spans `:1088-1145` (not `-1142`); `S4ClassName`
`:951-964`; the ⚙ note `:1500-1503`; the camelCase citation (`:712-713`, `:2332`) is the event-name
rule plus practice, not a stated field rule (row 26) — say "the contract's spelling convention"
rather than "normative for fields".

---

## What the CR's own weakness section missed

Its §12 is unusually honest (eight named weaknesses including its opponent's full argument and its
own correction history), but four things are absent from it, all found above: **(1)** the stale
schema `$comment`/vector gap — M1 — a contradiction its own §1 quote should have surfaced;
**(2)** the inactive-player item refusing §7.2's mandated shape — M2 — a structural bug in its
central deliverable; **(3)** the contract's internal `-32004`/`-32602` split on runtime-structural
bounds — its Q5 was framed as CR-vs-legacy when the harder tension is contract-vs-contract;
**(4)** §2.5's `"item 20's own scope"` clause — its Q6 framing ("settled only by a code comment")
undersells the contract text in its favor. And one slip inside its correction record (S1). None of
these defeats the design; all of them belong in §11.25's record.

## ⟨RUNTIME⟩ — for the controller's foreground follow-up (unchanged from the CR's §12.7, endorsed)

1. What the legacy server actually replies for both methods on a live aeon ROM (all §2.7 shapes here
   are source reads — mine as well as the CR's).
2. Whether `Object_RAM`, `ObjCodeBase`, `Dynamic_Slots`, `System_Slots`, `Effect_Slots`,
   `Object_RAM_End` all resolve in a current `s4.debug.lst` — D1's `pools` needs them; §5.1's
   optionality on `pools` is the fallback if any is absent.
3. `Player_1`'s address in a current build vs the demand's `$FF8DB0` vs the fixture's neighbourhood —
   either answer strengthens D1; the discrepancy is already proven documentary (row 37).
4. Whether consumer agent sessions call these tools today — no source sweep can count invocations;
   a shim log would convert §2.6's form-argument into a count.

Nothing in this ruling depends on any of the four.

## BLOCKED

Nothing was blocked. Items not reached, stated: the empyrean gate was **read but not run** (its G5
derivation is asserted from its source at `:217-223`, not from an execution — running it is the
landing commit's job under M1); aeon's `s4.debug.lst` was not read (per the CR's own §2.2 scope, and
the fixture excerpt stood in as the committed exhibit); no runtime item above was attempted; the
content of CR-A/CR-B/CR-C was not re-adjudicated (only §11.21/§11.23's landed text was read as
precedent). The sigil/oracle-old per-file sweep listings were verified as totals, not line-by-line
(aeon's, the load-bearing set, was verified to the line).

---

## Provenance

| | |
|---|---|
| Ruling written in | oracle worktree `/home/volence/sonic_hacks/oracle/.claude/worktrees/agent-ac166741392d5ab54`, branch `ruling-cr-d`, cut from `main` `8709790` |
| CR read at | worktree base = `24f400e`'s file, blob `2134864` at both `24f400e` and `8709790`; commits `1a7ae2b7a66531ddade50a8b6485f2e01c18ec1b`, `9cc293f963df8840920280a15aa9b16d2ff2a921` |
| Contract read at | empyrean `39cfaa27c293510d583581b5b07d07709691508a` and `78d432235090ae53848f4f6725f36ac148ff1ef4` (`origin/main` after fetch at ruling time) — `protocol.md` blob `b4776ce9…` and schema blob `7b24bced…` identical at both; also read there: `docs/2026-08-22-protocol-schema-audit.md`, `docs/ASSEMBLER_VISION.md`, `contract/schema/tests/validate_contract_schema.py` |
| Engine read at | aeon `f48961396848de666a737971bbb7b1c627a90f78` (`sst.emp`, `core.emp`, `ram.emp`, `constants.emp`, `objcodebase.emp`, `player_common.emp`, four `games/sonic4/objects/*.emp`, `tools/fixtures/s4_listing_excerpt.lst`); sweep at `b87e6e5a53d1e4ec45f2bdf614c663d5025e0eb7` |
| Legacy server read at | oracle-old `9dc67c5bb4e85b4c70e80e8a5b198f00d824877e` (`ControlSocket.cpp` whole; `oracle_mcp.py` by line) |
| Successor read at | oracle `0d7c5c21c2e92458d55de5d4a062e08d2532d610` (`engine.rs`, `tests/common/schema.rs`, vendored schema hashed, `docs/2026-08-22-peer-schema-defect-answers.md`, `docs/OVERSEER.md`, `docs/2026-08-26-obj-decode-demand.md`) |
| Consumers read at | aurora `e5dd32a56d4b4e11b5c28b614d14bba47bfdfd86`; sigil `e7f596eb436c537c7cd27e9b3120b38fed31c4c6`; seraph `2c8dc882aaddd4cf13618e1351f8c15bda002585`; `sonic_hack` `858af72c50083fa9e721ac1ecd69095022d3659e` (sweep + `S4.constants.asm`) |
| Runtime | none — no `cargo`, no emulator, no `mcp__oracle__*` tool, no server started; `docs/lane-status.json` untouched |
| Method | every sibling file via `git show <rev>:<path>` / `git grep <rev>`; every sweep count re-run with the CR's own patterns; the schema parsed with `json.load`, never regexed; full outputs read, none tailed |
