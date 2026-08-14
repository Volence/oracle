# Rulings on the five open decisions, round 2 (Fable, 2026-08-14)

**Commissioned by the owner** to adjudicate the decisions left open at the end of the checkpoint /
scanline-capture / size-filter arc on `trial-merge` (`19ba673`, 15 commits over `8e682ca`, unpushed).
Every load-bearing claim below was re-verified firsthand against the tree before ruling, per this
project's standing rule about restated figures (verification log at the end). Nothing outside this file
was modified. Format and standard follow the first rulings doc (`2026-08-14-fable-rulings.md`), whose
ruling F is the priority list this arc just finished executing.

---

## Summary for a non-specialist

Five decisions, five rulings, none deferred, plus a read on pushing:

| # | Question | Ruling in one line |
|---|---|---|
| **1** | Dropping a checkpoint id that doesn't exist — answer `removed: 0`, or refuse? | **Keep `removed: 0`.** Deleting something already gone is a satisfied request, not an error. Add one sentence to the contract so the next implementation can't decide it the other way. |
| **2** | Should the contract say how list-cursors must behave? | **Yes — but pin the *behavior*, not the mechanism.** One normative sentence: a cursor must survive concurrent deletes without skipping or repeating. How a server achieves that stays its own business. |
| **3** | Build the "expose the latches" item now? | **Yes — it is next in the queue and its yield condition never triggered. But land and push the current 15 commits first.** Don't stack a fourth slice on an unpushed pile. |
| **4** | Only one of four "machine must be paused" refusals is tested — enough? | **Pin all four, as one table-driven test.** This arc proved green suites can be weaker than they look; a wiring pin per entry point is the cheapest insurance there is. |
| **5** | Should mutation-testing new tests become standing practice? | **Yes — this was not a one-off, it was the base rate.** Scope it to evidence-bearing tests, done by the *author* at writing time, one recorded line each. The rule in five words: **never trust a test you haven't seen fail.** |
| **P** | Push the branch? | **Push it.** The gates are green (orchestrator-verified on `19ba673`; the parts I spot-checked firsthand agree). Fifteen unpushed commits containing two real bug fixes is risk with no offsetting benefit. |

Two of the brief's premises needed correction (§ "Premise corrections" below): the code cites a
"D13 rule 4" that does not exist in the contract, and one mutation statistic was slightly off. Neither
changes any ruling.

---

## 1. `checkpoint_drop` of an unknown id — `removed: 0` stands

**Ruling: keep `removed: 0`. Do not refuse. And close the judgement call permanently: add one
normative sentence to §6.1 saying so.**

**What it means in practice.** No code change. The owner adds to §6.1's cap paragraph something like:
*"`checkpoint_drop` of an unknown or already-dropped `id` succeeds with `removed: 0` — deletion is
idempotent. Only `restore` refuses unknown ids."* One sentence, filed in the same sitting as ruling 2's
amendment.

**Reasoning.**
- The implementer's hazard analysis is correct, and I verified its textual basis. The refusal norm for
  unknown ids is written for `restore` alone (§6.1: *"`restore` of an unknown or dropped `id` fails
  with `-32005` … never silently no-ops"*), and §6.1 describes drop's result as `removed` *"so the
  client knows how many actually went"* — which makes `0` a complete, honest answer, not a suppressed
  error. The hazard behind restore's refusal — an experiment proceeding against a machine the client
  didn't ask for — genuinely has no analogue for drop: after a drop-of-nothing, nothing was restored,
  nothing was evicted, no id changed meaning.
- The concurrency argument seals it. §6.1 explicitly expects two clients on one bus (it is the stated
  reason ids are server-assigned). Two clients racing to drop the same id is *normal*, and under
  refuse-semantics the loser of that race gets an error for a request whose intent — "make id N gone" —
  is fully satisfied. An error a client must learn to swallow trains clients to swallow errors.
- **Why the contract sentence is still needed:** the reviewer was right that this is a contract
  judgement, not a code one. Refuse-on-unknown-drop is a defensible reading a second implementation
  could ship in good faith, and then two conforming servers would answer the identical call
  differently — observable divergence is precisely what a contract exists to prevent. Per ruling E
  there is no foreign counterparty; the sentence costs one line of the owner's own reviewed spec.

**Premise correction.** The brief (and the shipped code comments, e.g. `engine.rs:1159/1316`, and the
test header at `checkpoints.rs:309`) cite **"D13 rule 4"**. D13 has **three** rules — I re-read it in
`empyrean/contract/protocol.md:136-155`. The restore-refusal norm the code means is the §6.1 sentence
quoted above. The implementer's *conclusion* is unaffected; the citation is drift from a draft schema.
Fix the comments whenever that file is next touched — not worth a commit of its own.

**What would change my mind.** A client observed using drop's `removed` as an existence test ("did it
exist?"). That is `checkpoint_list`'s job, and a client doing it via drop deserves an error it will
never get — but no such client exists, and designing for it would be inventing a user.

---

## 2. Cursor semantics — amend the contract, as an invariant, not a format

**Ruling: file the CR — but it must pin the *behavioral invariant* the shipped bug violated, not the
token's representation. The cursor stays opaque.**

**What it means in practice.** Add to §6.1's `checkpoint_list` paragraph (or generalize into §2's
bounded-array rule, scoped to queries over mutable collections):

> *"A `cursor` MUST remain valid under concurrent mutation of the underlying collection: resuming from
> it never skips an item that was live at both requests, and never delivers an item twice. (For queries
> over immutable per-call result sets — e.g. `lookup_symbol` — this holds trivially.)"*

Optionally a non-normative hint that monotonic never-reused server-assigned ids make this easy. Nothing
about what the token *is*.

**Reasoning.**
- The bug was real and I verified its whole shape in the tree: the positional cursor + `retain`
  compaction skipped a live checkpoint while reporting `truncated: false` (ledger,
  `2026-08-14-aether-change-requests.md:100-111`); the fix is id-keyed ("first id strictly greater
  than N", `engine.rs:1209-1241`, with the id-ascending assumption written down as a `debug_assert!`
  at `:1234`); and `rpc::bounded_array` was correctly left alone — `lookup_symbol`'s cursor really is
  a position into an immutable freshly-computed set, where the failure cannot occur.
- §6.1 already contains the sentence the bug violated ("a client must never be handed a partial list it
  can mistake for a complete one") — and a careful implementer *and* a reviewer still shipped the
  violating design past it. That is the empirical demonstration that the outcome-sentence alone does
  not carry the load: a second implementation can ship the identical bug and argue conformance, because
  its own `truncated` arithmetic is internally consistent. My CR-4 ruling stated the principle: *a
  contract that specifies a footgun is the source of truth for a footgun.* This contract currently
  *permits* one, which is the same defect one notch quieter.
- The over-specification worry is answered by where the line is drawn. Mandating "cursors are ids"
  would be over-specification — it would outlaw `lookup_symbol`'s perfectly sound positional cursor
  and constrain implementations for no client-visible benefit. Mandating *stability under concurrent
  mutation* constrains only what a client can observe, which is exactly the contract's jurisdiction.
- **Honest priority note:** with `cap = 8` and the default `limit` equal to the cap, a checkpoint list
  in practice always fits one page — the cursor path is nearly cold today. This CR is cheap insurance,
  not an emergency. File it in the same one-sitting contract pass as ruling 1's sentence.
- **Adjacent gap, same genus, noticed while verifying decision 4:** the contract's run-control state
  rule (`protocol.md:366-368`) names `run_to`, `run_to_scanline`, `run_frames`, `step*` — but
  oracle-aether also gates `press` and `reload_rom` on a paused machine, which the contract nowhere
  requires. A second server could accept `press` while free-running and both would conform. While
  editing §6.1 anyway, the owner should decide this on purpose: either add `press`/`reload_rom` to the
  named rule (my recommendation — both mutate the timeline, and the "never resolve wrong-state
  implicitly" principle applies identically), or record that their pause-gating is
  implementation-defined.

**What would change my mind.** If the suite ever wanted cursors that intentionally tolerate loss
(e.g. a lossy log tail), the invariant would need a carve-out — but `log_tail` already uses a different
mechanism (`since` tokens), so the conflict is hypothetical.

---

## 3. `F-TRACE-EXPOSE-LATCHES` — build it, after the branch lands

**Ruling: yes — it is simply next in the queue. My ruling F ranked it item 4 behind three items that
are now done, and its one yield condition (the Aeon replay-runner starting) has not triggered — the
recon still lists that as planned P3 work. But sequence it: review and push the current 15 commits
first, then cut this as a fresh slice.**

**What it means in practice.** Read-only accessors — `System`-level for the two arbiter latches,
`Ym2612`-level for the address latch — plus tests written under ruling 5's discipline. No new
`BusEvent` fields, no sink API changes.

**Evidence re-verified.** All three shadow reconstructions are real and current:
`K4Probe` shadows `z80_busreq`/`z80_running` from the write stream (`examples/k4_openbus_probe.rs:47-52`);
`VgmLogger` keeps `fm_addr_latch` (`vgm.rs:295`); `AudioSink` keeps `fm_addr_latch: [u8; 2]`
(`synth/audio_sink.rs:46`). The design ledger's census note stands: ~11 of `K4Probe`'s 16 counters are
watch-expressible, and the latch accessors — not more census primitives — are what the rest needs.

**One scope caution the census claim glosses over.** "Deletes 3 shadow reimplementations" oversells by
two. `VgmLogger` and `AudioSink` are *sinks* — they are handed events, not the machine — so they cannot
call a `System`/`Ym2612` accessor mid-stream, and their `fm_addr_latch` is decode state on a path whose
VGM export golden is byte-frozen. Leave both alone. The realistic deliverable is the accessors
themselves plus the `K4Probe` simplification (the probe *drives* the system and can read latches at
frame boundaries). That is still worth building — the accessors are what the *next* bus-arbitration
hunt reaches for — but the slice should be scoped and its commit message written to the honest count:
one deletion, three consumers-in-waiting.

**Why "after the push" is part of the ruling.** Fifteen unpushed commits already include two real bug
fixes other work could collide with. Every additional slice stacked locally raises the cost of the
owner's eventual review and the blast radius of a lost working copy. The item is cheap and
non-urgent-by-construction ("its payoff arrives on the next hunt"); nothing is gained by racing it onto
an unlanded branch.

**What would change my mind.** Same as ruling F: the Aeon replay-runner starting outranks it. And if
the next hunt arrives *before* the accessors exist, build them then — they are a half-day slice.

---

## 4. One tested refusal path out of four — pin all four

**Ruling: pin each of the four `require_paused` wirings on the wire, as one table-driven test. And
adopt the general principle: a shared helper's *logic* is tested once, thoroughly; its *wiring* into
each public entry point is pinned once per entry point whenever the behavior is externally promised.**

**What it means in practice.** One test in `crates/oracle-aether/tests/methods.rs`: iterate
`[(method, minimal_valid_params)]` over `run_frames`, `run_to`, `press`, `reload_rom`; while
free-running, assert each returns `-32005` with `data.reason = "machineRunning"`; after `pause`, assert
each succeeds. Roughly twenty lines, one spawn.

**Verified state.** `require_paused` has exactly four callers and is the first statement in each
(`engine.rs:606/630/802/1040` — confirmed by reading the four handler heads). Exactly one is wire-tested
(`methods.rs:500-514`, `run_frames`). The contract's run-control rule names `run_to` explicitly
(`protocol.md:366`), so an untested contract-named behavior is currently vouched for by nothing but
source-reading.

**Reasoning.**
- The structural-inheritance argument is true *about today's source text* and silent about tomorrow's.
  The regressions that would break it — a refactor that hoists param parsing above the gate (changing
  which error a running machine gets), a new run-shaped method that forgets the call, an overzealous
  cleanup that inlines and drops one call site — are exactly the class no existing test can see. This
  arc's record is three-for-three that "no test can see it" is a live condition, not a hypothetical.
- The litmus test for wiring pins is mutation-shaped: *would deleting this one call site fail any
  test?* Today, for three of the four sites, the answer is no. After the table-driven test, yes for
  all four — at the cost of one test.
- **The general principle, stated for reuse:** (a) test the helper's logic once, where it lives;
  (b) pin the wiring once per public entry point *when the entry point's behavior is promised
  externally* (contract-named, ledger-cited, or golden); (c) never multiply states × methods — the
  wiring pin is one row, not a matrix. Structural inheritance is an implementation technique, not a
  coverage argument.
- Note from ruling 2: two of the four gates (`press`, `reload_rom`) are house policy the contract does
  not currently name. Pin them anyway — they are observable wire behavior clients will come to rely
  on — and let the owner's contract pass decide whether to promote them to named rules.

**What would change my mind.** Nothing at this price. If the catalog someday had fifty gated methods,
the table would grow rows, not tests — the form scales.

---

## 5. Mutation verification — adopt it; this was the base rate, not bad luck

**Ruling: yes, standing practice — scoped to evidence-bearing tests, executed by the author at writing
time, recorded in one line each. Not every test; and not a standing second adversarial review round.
The rule in one sentence: a test that has never been seen to fail is not evidence, and this project
runs on evidence.**

**The record that forces the ruling.** Three independent slices, different agents, all green on tests +
clippy + currency, all through review — and each contained a vacuous test found only by mutation:
`fs::write` inside the checkpoint handler left the whole suite green (ledger,
`aether-change-requests.md:119-125`); the scanline retention oracle ran on `testrom::build()`'s
constant all-black picture and passed a first-frame-for-last mutation (design ledger, `:1053`); three
parity-filter tests used parity-balanced streams and were blind to a total inversion of the filter
(`12fa976`'s F3). Separately, the orchestrator was personally fooled by an `!is_empty()` oracle. When
every slice in an arc exhibits the failure mode past every gate you own, it is not an anomaly — it is
the measured error rate of the workflow. Declaring it a one-off would be the exact epistemic mistake
the arc keeps catching.

**Scope — where mutation is mandatory:**
1. Any test **cited as evidence for a claim** — in a commit message, a ledger row, a review verdict, or
   as a "money test". If a sentence says "pinned by a test", the pin must have been seen to hold weight.
2. Any test guarding a **fixed bug** (the mutation is the bug: re-introduce it, watch the test fail).
3. Any **anti-vacuity control** (a control that has never fired is itself unverified).

Plumbing and shape tests (JSON field present, error code numeric, builder returns self) are exempt —
mutating those buys nothing and the tax would erode compliance with the part that matters.

**Form — authoring time, not review time.** The author, before requesting review: apply the targeted
mutation (invert the exact behavior the test claims to pin — not a random mutation), observe the
failure, revert, and record one line: *"mutation: <what> → <N> tests fail; reverted."* `12fa976`'s F3/F4
entries are already the house format ("Verified by applying that exact mutation: all three now FAIL …
reverted, all green"; "Verified by adding a `Size::Quad` variant: `error[E0004]` … Reverted"). This is
TDD's red step, retrofitted for a project that writes tests after implementation — either discipline
satisfies the rule; a test born red and watched turn green needs no separate mutation.

**The cost accounting, faced squarely.** The review+fix round roughly doubled this arc's wall-clock —
but that was the cost of *retrofitting* the check as a separate adversarial pass with its own agent,
its own reading-in, and a fix round after. An author mutating their own fresh test is minutes: the code
is in their head and the harness is already warm. Adopting the authoring-time rule is what lets the
expensive adversarial round shrink back to slices that touch currency or contract surface — it is the
cheap substitute for the thing that was expensive, not an addition to it.

**What would change my mind.** The rule should pay rent like everything else: if after, say, ten slices
the recorded mutation lines have caught nothing (every test failed its mutation first try), downgrade
to spot-checking evidence-cited tests only. The ledgers already have the format to track it.

---

## P. The push

**Push it.** Not my call to make — but the owner asked for a read, and the read is unambiguous. The
gate figures (full suite 1154/0 over 24 legs, clippy zero warnings, 21/21 currency literals) are the
**orchestrator's own run on this exact tip (`19ba673`)**, taken on report; everything I spot-checked
firsthand from this fresh worktree agrees — 15 of the legs green before I stopped a redundant re-run
(including the 830-test core leg), the `oracle-frontend` leg 52/0, `cargo fmt --all --check` clean,
and zero hash-literal movement in the 15-commit diff over the three currency files. The branch
contains two genuine bug fixes (the cursor skip, the symbols-across-restore staleness) and the live
§6.1 implementation; every day it sits local is a day a lost working copy costs real reviewed work,
and nothing pending in this ruling set modifies any of its commits.

Two operational notes from re-running the gates, both already in the project's ops lore and both bit me
anyway, so they evidently need the repetition:
- **`cargo test --workspace | tail` reported exit 0 over 8 real failures** (the pipe returns `tail`'s
  status). Capture to a file and check `$?` of `cargo` itself.
- The 8 "failures" were the **fresh-worktree `vendor/` footgun**: `vendor/` is gitignored, and
  `oracle-frontend`'s save-state tests need `vendor/TestRoms`. With the symlink in place the same leg
  is green. Any CI or clean-clone gate must create that symlink or run the fetch script first.

---

## Premise corrections (the brief's facts vs the tree)

1. **"D13 rule 4" does not exist.** D13 (`protocol.md:136-155`) has three rules; the unknown-id refusal
   norm for `restore` is a sentence in §6.1's restore paragraph. The shipped code comments and one test
   header cite "rule 4" — drift from a draft, harmless, conclusions unaffected (ruling 1).
2. **The parity mutation figure.** The brief says the inverted filter left "3 of 6 tests passing";
   `12fa976`'s own record says three tests were blind while two others caught it — 3 of 5 synthetic-
   stream tests, before counting the real-machine tests that also caught it post-F2. Same substance,
   slightly different denominator.
3. **The gate figures needed one environmental caveat.** "1154 passed / 0 failed / 24 legs" is what a
   worktree *with the vendor symlink* produces. A clean clone without it fails 8 `oracle-frontend`
   save-state tests on missing `vendor/TestRoms` ROMs — observed firsthand in this worktree before the
   symlink existed (44/8 on that leg), green after (52/0). The orchestrator's figures were honest; the
   caveat travels with them.

---

## Verification log (what was checked firsthand for this ruling)

- Worktree cut from `8e682ca`; `git merge-base --is-ancestor HEAD trial-merge` succeeded with a clean
  tree; `git reset --hard trial-merge` → `19ba673`; `git rev-list --count` confirms **15 commits** over
  `m68000-microop-framework`.
- `checkpoint_drop` returns `removed: before - after` with the unknown-id-is-answered comment
  (`engine.rs:1294-1319`); wire test asserts `removed: 0` for both `all`-on-empty and unknown id
  (`tests/checkpoints.rs:606-614`) — confirmed.
- §6.1 full text and D13's **three** rules re-read in `empyrean/contract/protocol.md` (§6.1 at
  `:463-508`, D13 at `:136-155`); restore-refusal sentence found in §6.1, not in any "rule 4" —
  confirmed.
- Id-keyed cursor implementation + `debug_assert!` on id-ascending order (`engine.rs:1206-1292`);
  `rpc::bounded_array` unchanged and shared with `lookup_symbol` — confirmed in code and ledger
  (`aether-change-requests.md:100-111`).
- `require_paused`: exactly 4 callers, first statement in each (`engine.rs:606/630/802/1040`); exactly
  one `machineRunning` wire test in the tree (`tests/methods.rs:500-514`) — confirmed by grep over all
  test files. Contract run-control rule names `run_to` (`protocol.md:366-368`) and does **not** name
  `press`/`reload_rom` — confirmed.
- Latch shadow anchors: `k4_openbus_probe.rs:47-52` (`z80_busreq`/`z80_running` shadows), `vgm.rs:295`
  (`fm_addr_latch`), `synth/audio_sink.rs:46` (`fm_addr_latch: [u8; 2]`) — all three confirmed; the
  brief's line numbers (49/291/43) had drifted by a few lines each.
- Vacuous-test ledger entries: `fs::write` mutation (`aether-change-requests.md:119-125`), all-black
  retention oracle + `LastFrame` resync (`trace-recorder-design.md:1053`), parity-inversion blindness
  + exact-mutation re-verification (`git show 12fa976`, findings F1-F6) — confirmed.
- Aeon replay-runner: still listed as planned P3 glue in `2026-08-14-tooling-frontier-recon.md:357/431`;
  no runner work in the tree — yield condition not triggered.
- Gates. **Taken on the orchestrator's report, verified by them on this exact tip (`19ba673`):**
  `cargo test --workspace` exit 0 / 1154 passed / 0 failed / 24 legs; `cargo clippy --all-targets
  --workspace` exit 0, zero warnings; 21/21 pinned currency literals byte-identical. **Checked
  firsthand in this worktree at `19ba673`:** 15 workspace legs green (including the 830-test core
  leg) before I stopped a redundant full re-run on the coordinator's instruction; `cargo test -p
  oracle-frontend` exit 0, 52 passed / 0 failed (after the `vendor` symlink; 44/8 before it — the
  documented fresh-worktree footgun); `cargo fmt --all --check` exit 0; and `git diff 8e682ca..HEAD`
  over `conformance_roms.rs` / `golden_frames.rs` / `testrom_probe.rs` touches **zero** hash literals
  (underscore-aware grep over the diff: no matches) — the currency claim confirmed independently at
  the source level.
