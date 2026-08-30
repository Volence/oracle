# Un-blinding the replay net — the pin moved to chain 189, and the playthroughs now run

**Date:** 2026-08-30 · **Branch:** `parcel/replay-net-unblind` (worktree off `main`)
**Acts on:** `docs/2026-08-30-restamp-ab-chain189.md` (the measurement) and the `REPLAY-NET-BLIND-3`
queue item in `docs/OVERSEER.md`.

---

## The problem, stated so the fix cannot be misread

Two blindfolds were stacked, and removing either alone left the net blind:

1. the three playthroughs that would catch a replay regression were `#[ignore]`d, so only a
   `--ignored` flag nobody types could run them;
2. the fixture they run against was **itself stale** — pinned at aeon chain 186, whose embedded
   `ojz_fixture` stream disagrees with what the machine produces at checkpoints 18–26 of 27.

Un-ignoring the tests and reporting the net fixed would have produced a green suite that is still
blind. Both were removed, in that dependency order.

---

## What changed

On `parcel/replay-net-unblind`, worktree off `main` at `cbf4d87`.

| item | commit | files |
|---|---|---|
| 0 — the repair | **`ebe3df76`** | the six `fixtures/aeon/` artifacts, `fixtures/aeon/PROVENANCE.md`, `fixtures/aeon/PIN.tsv` (new) |
| 3 — name the chain | **`e389a79e`** | `crates/oracle-replay/tests/aeon_pin.rs` (new), `crates/oracle-core/tests/symbols_real_lst.rs` |
| 2 — the reporter | **`acb1697a`** | `tools/aeon_pin_report.py` (new) |
| 1 — the runner | **`08e7aa0a`** | `crates/oracle-replay/tests/replay_real_artifacts.rs`, `tools/replay_playthroughs.sh` (new), `.github/workflows/ci.yml` |
| — this report | *(the commit that adds this file)* | `docs/2026-08-30-replay-net-unblind.md` |

Item 3 landed before item 2 in commit order, because the gate reads `PIN.tsv` and the reporter reads
the same file; the dependency runs data → gate → reporter → runner (the runner script invokes both).

---

## Item 0 — the pin moved to chain 189 (option **A**, full re-pin)

### The decision, and what decided it

**(A) full re-pin — all six artifacts at chain 189.** It was reached in two steps, and the first step
is worth recording because it was the honest answer at the time.

At the start of this parcel, `(A)` was **BLOCKED**: chain 189's three other listings did not exist.
`/home/volence/sonic_hacks/.aeon-chain189` existed and was clean at `3f143178` — but held **no build
outputs at all** (`find … -name '*.lst' -o -name '*.bin'` returned nothing), and building in aeon's
repo was outside this lane's permission. `(B)` pairwise was drafted and half-landed on that basis.
aeon then built in that worktree and the block lifted; `(A)` landed over it.

The `(B)` draft left one thing behind that is worth keeping: **the pin is now recorded per file**
(`PIN.tsv`), not per directory, so a future partial move is expressible without a format change. It
happens to be uniform today.

### The pin, re-derived rather than accepted

Every fact below was re-derived here. **No disagreement with what was relayed.**

| claim | how derived | result |
|---|---|---|
| chain 189 = sigil `39c34fd2` | `[[entry]]` count in `provenance.toml` at that rev | **189** |
| …and it is an ancestor of `origin/master` | `git merge-base --is-ancestor` | YES, 26 commits back |
| chain number is derivable at all | same count at `5af70797` → **186**, at tip `3ad7ed02` → **189** | 3 for 3 |
| golden `s4.debug.bin` blob | `git rev-parse 39c34fd2:…/s4.debug.bin` | `ab8661bd…` ✓ |
| golden `s4.bin` blob | same | `34c8655d…` ✓ |
| **goldens are current at TIP** | same blob ids at `3ad7ed02` | **identical** — 26 commits of sigil movement, zero golden movement |
| sigil freezes no listings | `ls-tree -r` of the golden dir at `5af70797`, `39c34fd2`, **and tip** | zero `.lst` at all three |
| tip chain entry | last `[[entry]]` at tip | `replay-restamp-all-ten`, `aeon_rev 3f143178` — **chain 189 IS sigil's tip chain** |

The currency question was asked **at tip**, not at the pinning revision, because a pinned blob equals
itself by construction and a drift check aimed there passes for the wrong reason forever.

### The artifacts, and their authority

ROMs from **sigil's committed golden blobs at `39c34fd2`** (the authority). Listings from
`/home/volence/sonic_hacks/.aeon-chain189`, a detached aeon worktree at `3f143178`, `git status
--porcelain` empty, built 06:06–06:11.

```
05c2738d759a119dde2d7c799b51fceff2e2c07a89a4cd2cf87adae60e62169c  s4.bin          719315
4ee7ac79737f1decc16c13cef4e160ed26c3fea078b3f5b2b7c4300857a9a0b3  s4.debug.bin    736315
b3e1dc424c209a643761dcf4133a9bbf7b18d602e8cdaa51a86db2517e9a48fe  s4.lst          280720
81a111020e3f28ddda374648e7a3e1425cbde00ce5b09ea5769b83eb79845a2f  s4.debug.lst    330541
46059a06b963ed00f4350f10eaf6ccc3ca81012a82c67c8808363f1c73d14fbb  demo.lst        176191
6e28e3014ca1c563b7177d05718181ebc588d44b854d48d1e688c3c7eae62cdb  demo.debug.lst  204583
```

**The identity control, widened and re-derived.** All **four** on-disk ROMs in that build tree were
hashed against sigil's committed goldens at `39c34fd2` — not just the two we froze:

```
05c2738d…62169c  s4.bin          ==  golden 39c34fd2
4ee7ac79…a9a0b3  s4.debug.bin    ==  golden 39c34fd2
426d43ed…c883c0  demo.bin        ==  golden 39c34fd2
938cb954…2a012   demo.debug.bin  ==  golden 39c34fd2
```

Four for four. And `s4.debug.lst` is **byte-identical** to `/home/volence/sonic_hacks/restamp-ab-chain189/`'s
copy, taken an hour earlier by a different lane — the control that mattered, since aeon's *live* tree
holds a 330588-byte `s4.debug.lst` that is a genuinely different file.

**The joint**, proved by the runner rather than by the directory:

```
lst: 2743 symbols, bound to this ROM (deb2 appendix at $0A7F40, 48379 bytes)
PASS — the stream ran to its end, corroborated three ways.
```

`$0A7F40` matches sigil's own `provenance.toml` (`s4_debug anchor_end = 0xa7f40`), and
`48379 = 736315 − 0xA7F40`.

**Tick counts unchanged** — checked, not assumed. `Fixture::Ojz` declares 1721 (reached 1723),
`Fixture::OjzSlide` declares 2350 (reached 2352). Both pinned constants stand; no re-derivation needed.

### ⚠ NARROWED, NOT CLOSED — the assembler

Recorded in full in `fixtures/aeon/PROVENANCE.md`; the short form: chain 189 records
`sigil_rev = 5552bccb`, the build that produced these listings reported `d5967f87-dirty`. Four
byte-identical ROMs say the two agree on everything that reaches a ROM — **a listing is not a ROM**.

Verified here rather than accepted:

* **`5552bccb` is an ancestor of `d5967f87`, 19 commits.** 13 files: 5 docs, 8 code/data.
* **`seam2.rs` IS the sound emitter module** — `emit_sound_blob.rs` is a thin argv wrapper over
  `seam2::emit_{dac,mt,sfx,seq_opcode,sound_tables,pitchtable}_artifacts`, six call sites at
  `d5967f87` lines 70/76/82/88/93/99. Confirmed firsthand.
* **Strip doc comments from `seam2.rs`'s +56 and exactly two code additions remain** — confirmed
  firsthand by filtering the diff: a new `require_named_reference_tree()` (an `AEON_DIR` presence
  check returning `Ok(())` or an error naming the fallback and the refused target), and one line
  calling it. **It adds a refusal. It changes no emitted byte.**
* **`sigil-cli/src/main.rs`** (+13) and **`emit_sound_blob.rs`** (+12) each publish `--aeon` into
  `AEON_DIR`. Read, not inferred.
* **Irreducible residue:** `-dirty` is unrecoverable.

**One disagreement with the relayed wording, reported rather than reconciled.** The final relayed
sentence enumerates three pipeline-reachable files. I make it **four**:
`crates/sigil-harness/src/test_support.rs` (+18) hoists the fallback path into a named constant
`LIVE_TREE_FALLBACK`, which `seam2.rs` resolves and names in its refusal text (`seam2.rs:99,100,127` at
`d5967f87`). It is a literal-to-constant hoist with identical resolution behaviour, so **the conclusion
is unaffected** — but the enumeration is incomplete without it, and this is the third round in which a
file was misfiled by its *name* (`test_support`) rather than its *role*. `PROVENANCE.md` lists four.

The `tests/…` three are excluded on a **structural** argument — a cargo integration-test target cannot
be linked into a binary — not on their directory name.

**What does NOT narrow it**, and this is the half worth carrying: the byte-identity between these
listings and the earlier snapshot is aeon's build against aeon's build. If both ran the same
non-chain-189 assembler they agree and prove only reproducibility. It is a control against taking the
*wrong file*, not a control on the *toolchain*. The committed-diff check is what narrows it, because it
enumerates over a different parameter entirely.

**aeon's measurement, credited to them and not re-derived here:** under chain 189's own assembler an
unset `AEON_DIR` fell back to the live checkout whose output dir is gitignored, so a stray write left no
trace; the newer binary refuses instead, and it fired during this build —
`engine/sound/generated/` in the live checkout stamped 05:52:32 and untouched, the same directory inside
`.aeon-chain189` stamped 06:11:11. On the one axis where the assembler differs, it is the **safer** one.
Our conclusion rests on the diffs regardless.

---

## THE QUESTION FOR ACROSS THE FENCE

> *Is the repair byte-for-byte the chain-189 payload set, or something narrower?*

**Neither. It is a pin move, not a payload patch — and the artifact that resulted is byte-for-byte
chain 189's entire published set, which is strictly wider than the nine payloads.**

The nine repair payloads were known byte-for-byte and were **not applied**. Patching our frozen
chain-186 ROM would have produced an artifact belonging to no chain, and attributability is the whole
reason `fixtures/aeon/` exists. What we hold is sigil's committed chain-189 goldens, taken as git blobs.

Evidence that this is wider than the nine, and it is a byte search rather than an argument:

| image | the nine STALE payloads | the nine REPAIRED payloads |
|---|---|---|
| chain-186 `s4.debug.bin` (was ours) | present, `$0A6CDC…$0A6D30` | absent |
| chain-189 `s4.debug.bin` (now ours) | absent | present, same offsets |

`s4.debug.bin` went 736,095 → 736,315 bytes and `s4.bin` 719,235 → 719,315, so the images differ by far
more than 36 bytes of payload — chain 186 → 189 spans three freezes, not one re-record. **Chain 188 →
189 alone is a pure fixture re-record; chain 186 → 189 is not.**

---

## The release-`s4.bin` measurement, and how it was settled

**Question:** does the release `s4.bin` carry the replay fixture stream at all, or is it debug-only?
This decided how much staleness a pairwise `(B)` move would have left behind.

**Answer: it carries the whole stream. What it lacks is the *compare*.** Settled from source and from
the bytes, both:

* **Source.** `crates/oracle-replay/src/policy.rs` records that `aeon/engine/system/replay.emp:174-186`
  gates the checkpoint compare on `DEBUG == 1`; the release shape *steps over the payload without
  comparing*. That is why `require_debug_rom` refuses a release ROM — it would run the stream to the end
  and report a green having verified nothing.
* **Bytes.** All nine payloads were searched for in all four images:

| image | nine STALE | nine REPAIRED | `REPLAY DESYNC` |
|---|---|---|---|
| chain-186 `s4.bin` | **present**, `$0A4A2C…$0A4A80` | absent | no |
| chain-186 `s4.debug.bin` | present, `$0A6CDC…$0A6D30` | absent | yes |
| chain-189 `s4.bin` | absent | **present**, same offsets | no |
| chain-189 `s4.debug.bin` | absent | present | yes |

* **Corroborated from sigil's own chain-189 entry**, which records that the `s4` **release** golden
  moved in the restamp (`6e2f9b22 → 63451f96`) while `demo` and `demo.debug` did not: *"the demo shapes
  carry no fixture, which is why they must not move."*

**Consequence at the time it mattered:** `(B)` pairwise would have left nine stale payloads sitting in
our `s4.bin`. Inert — no test replays a release ROM — but stale, and *"release carries no fixture"* was
the convenient answer that would have made `(B)` free. It is false. Since `(A)` landed the point is
moot; it is recorded so the next partial move is argued against the bytes.

---

## Item 1 — the playthroughs run somewhere deliberate

**Three** playthroughs, not two (the third at `replay_real_artifacts.rs:716`, ~100 s debug, which
neither lane's booking had named).

**What changed:** `#[ignore = …]` → `#[cfg_attr(debug_assertions, ignore = …)]` on all three.

* `cargo test` / `cargo test --workspace` (debug) — **skipped**, exactly as before. The default suite's
  runtime is untouched; *a suite that slow gets reverted by the first person it annoys*.
* `cargo test --release` — **they run**. No `--ignored` flag to remember, and no way to get a green out
  of that command while the net is switched off.

That distinction is the point. A plain `#[ignore]` is opt-in by a flag nobody types, which is exactly
how the pin was allowed to go stale under a green suite.

**THE RUNNER THAT ACTUALLY EXECUTES THEM** — named, as required, and it is two things that are the same
command:

* **CI job `replay-playthroughs`** in `.github/workflows/ci.yml` — its own job (not appended to `Test`),
  gated behind `determinism-gate`, running `./tools/replay_playthroughs.sh`.
* **`tools/replay_playthroughs.sh`** locally — the same script, so there is no drift between what CI
  runs and what a developer runs. Exit status is the test run's; the pin report it prints afterwards is
  REPORT ONLY and cannot change it.

---

## Item 2 — the pin's staleness is visible, without a gate

`tools/aeon_pin_report.py`. **It asks the CURRENCY question — "has it moved?" — and therefore asks it
at TIP.** It always exits 0. Nothing calls it from a gate.

**Why not a test, argued against the constraint rather than around it.** A default-suite test reading
sigil's golden at `origin/master` is wrong twice: it reintroduces the sibling-checkout dependency the
freeze exists to remove (our suite would stop passing in a fresh clone and on CI), and it makes our
build red because *someone else* moved. When a gate goes red, "the consumer is broken" is a conclusion
requiring work from nobody except the consumer, and the gradient then pushes toward bending our side
until it goes green — which is precisely how a pin gets moved to make a red test pass. So it is read,
not run as a gate. I do not think a gate is right here.

**The complementary recovery question** — *are the bytes here the bytes we recorded?* — **is** a gate
(item 3), because it is a fact about this repository alone and can never redden because of somebody
else's commit.

What it does per file (so it survives the pin becoming mixed or un-mixed without a rewrite):

* resolves sigil, then `origin/master`, printing the tip SHA and its commit date **and warning that the
  local mirror may itself be behind** (`--fetch` to update; it does not fetch on its own, because a
  reporter should not mutate);
* derives the tip chain number from the `[[entry]]` count and prints the tip freeze entry's name;
* for the two ROMs, hashes the tip blob and reports AGREES / DIFFERS, printing both hashes and both
  lengths — and, when lengths match but bytes do not, saying so explicitly, because *byte-count-neutral
  is not byte-identical*;
* for the four listings, prints **UNMEASURABLE**: sigil freezes no listings at any revision checked, so
  their currency is not measurable from sigil at all. They are never counted as agreement.

**Loud on unmeasurable, proved:** with no sigil checkout it prints
`UNMEASURABLE: no sigil checkout found … This is NOT 'the pin is current'. Nothing was compared.` and
exits 0. Same for an unresolvable ref. Both paths were exercised.

Live output at the end of this parcel: **2 agree, 0 differ, 4 unmeasurable, over 6 pinned artifacts** —
against sigil tip `62691b84`, chain 189.

---

## Item 3 — the suite names the pinned chain

`crates/oracle-replay/tests/aeon_pin.rs`, in the **default** suite. It reads `fixtures/aeon/PIN.tsv`,
hashes every artifact, checks the manifest lists exactly the files present, and prints:

```
=== FROZEN AEON PIN: aeon chain 189 ===
  file              bytes  chain sigil     aeon_rev  authority
  s4.bin           719315  189   39c34fd2  3f143178  sigil-golden-blob
  s4.debug.bin     736315  189   39c34fd2  3f143178  sigil-golden-blob
  s4.lst           280720  189   39c34fd2  3f143178  aeon-build-tree
  s4.debug.lst     330541  189   39c34fd2  3f143178  aeon-build-tree
  demo.lst         176191  189   39c34fd2  3f143178  aeon-build-tree
  demo.debug.lst   204583  189   39c34fd2  3f143178  aeon-build-tree
  4 of 6 rows have NO upstream counterpart (sigil freezes no listings), so their currency is not measurable from sigil at all.
  A green here is a statement about these bytes ONLY. Whether aeon's master has moved past them is a separate, non-gating question: tools/aeon_pin_report.py
  The rest of the suite is running against these bytes.
```

It prints `MIXED — aeon chains N and M` when the rows disagree, and — the anti-blindfold detail — when
`ORACLE_AEON_DIR` is set it says out loud that *the rest of the suite is NOT running against the pin it
just named*. It reads the frozen directory directly, never the override, so the override cannot make it
vacuously red.

**⚠ A harness limit, found by checking the output rather than assuming it.** libtest **captures a
passing test's stdout**, so under a plain `cargo test --workspace` this banner is printed and then
swallowed — visible only on a *failure*, the one case where naming the pin matters least. Confirmed by
grepping the full workspace log: zero occurrences of `FROZEN AEON PIN`. So the chain is named by running
this one file with `--nocapture` as its own step, twice over: the `Name the frozen aeon pin` step in
`.github/workflows/ci.yml` (before the workspace `Test` step), and the first thing
`tools/replay_playthroughs.sh` does. Both cost well under a second. A third place the suite runs must
name the pin too, or that run's green says nothing about which build it passed against — noted in the
test file's own docs so the next person adding a runner sees it.

`crates/oracle-core/tests/symbols_real_lst.rs` gained
`the_frozen_pin_this_file_reads_is_named_in_the_output`, which **prints** the manifest and does not
judge it — the hashing authority lives in one place, not two, because `oracle-core` is a no-I/O crate
with a single dependency and has no business owning a provenance parser.

**SHA-256 is implemented in the test file**, not pulled in, because `oracle-replay` carries exactly one
dependency as a stated crate property. It is proved before it is trusted, against the FIPS 180-4
vectors — including the empty-input vector `e3b0c442…`, which is what a shell pipeline returns when the
command feeding it failed to stderr and hashed nothing, and which has been mistaken for a real artifact
hash in this workspace more than once.

---

## How verified

### Red-first evidence for everything added

**The pin gate, failure 1 — a fixture byte changed without the manifest.** One byte of
`fixtures/aeon/s4.bin` was flipped at offset 1000:

```
assertion `left == right` failed: s4.bin does NOT match the pin.
  on disk  ae0630a401cf1c2e3160588c1029df50b76a5324693fe3ef017fe63947ecb9cd
  PIN.tsv  b0873bed491c16b97f0cd1a1e7dba0acbebdb8e55276e2fd092b0e1705be3351
If this artifact was moved on purpose, PIN.tsv and PROVENANCE.md move with it, in the same commit.
The pin never moves to make a red test go green.
```

Restored and re-hashed to the committed value; `git status` clean on that path.

**The pin gate, failure 2 — an artifact present but unlisted.** `fixtures/aeon/stray.bin` was created:

```
assertion `left == right` failed: PIN.tsv must list exactly the artifacts in …/fixtures/aeon
  left:  ["demo.debug.lst", "demo.lst", "s4.bin", "s4.debug.bin", "s4.debug.lst", "s4.lst", "stray.bin"]
  right: ["demo.debug.lst", "demo.lst", "s4.bin", "s4.debug.bin", "s4.debug.lst", "s4.lst"]
```

Removed; green restored.

**The reporter's unmeasurable paths** (it is non-gating, so "red-first" means *proving it refuses to
render a non-measurement as agreement*): `--sigil /nonexistent/sigil` prints
`UNMEASURABLE … This is NOT 'the pin is current'. Nothing was compared.`, and
`--ref refs/heads/no-such-ref` prints `UNMEASURABLE: … does not resolve in that checkout.` Both exit 0.
A live run additionally produced one genuine `DIFFERS` (mid-parcel, `s4.bin` at chain 186 against a
chain-189 tip) and four genuine `UNMEASURABLE`, so both non-agreement verdicts are exercised on real
data, not only on synthetic inputs.

### The staleness this repairs, reproduced before the fix

Against the **old** chain-186 pin, wall clock 10:06:18Z → 10:06:22Z:

```
DESYNC — a checkpoint did not match.
  Logic_Tick 1154   expected $A6CC0AEB   actual $607BF6C4
  raised at $002718  (Input_Tick.desync+$4)
EXIT=2
```

Against the new chain-189 pin, same command, 10:06:28Z → 10:06:32Z: `PASS`, `Logic_Tick 1723 >= 1721`.
Slide fixture 10:06:44Z → 10:06:50Z: `PASS`, `Logic_Tick 2352 >= 2350`.

### Test totals — aggregated, with the ignored named

**`cargo test --workspace` (debug), after the pin move:**

> **62 suites · 1992 passed · 0 failed · 6 ignored · 0 measured.**

Aggregated across every `test result:` line in the run, not read off a tail. The 6 ignored, by name:

| test | why |
|---|---|
| `dry_run` | dry-run scaffolding; needs `AETHER_DRYRUN_SCHEMA` — pre-existing |
| `the_probe_rejects_a_reply_the_candidate_forbids` | same — pre-existing |
| `tests::write_presentation_screenshots` | needs a ROM via `ORACLE_SHOT_ROM` — pre-existing |
| `the_standing_fixture_runs_green` | **~34 s unoptimized — runs under `cargo test --release`** |
| `the_slide_fixture_runs_green` | **~49 s unoptimized — runs under `cargo test --release`** |
| `one_pass_repairs_four_stale_checkpoints_and_reproduces_the_pristine_image` | **~100 s unoptimized — runs under `cargo test --release`** |

**`cargo test --release -p oracle-replay`** — the lane that proves the un-ignoring works:

> **93 passed · 0 failed · 0 ignored** (75 + 0 + 2 + 16 + 0 across five targets).

Zero ignored. All three playthroughs ran and passed:

```
ojz_fixture: PASS — armed at frame 34, 1790 frames after the arm, Logic_Tick 1723 (stream declares 1721)
ojz_slide_fixture: PASS — armed at frame 34, 2415 frames after the arm, Logic_Tick 2352 (stream declares 2350)
4 of 27 checkpoints re-stamped in one pass; the result is byte-identical to the pristine image and runs
green with the control still tripping
```

**`cargo test --release -p oracle-core --test symbols_real_lst`:** 10 passed · 0 failed · 0 ignored,
printing the chain-189 banner.

**`cargo fmt --all -- --check`** clean · **`cargo clippy --all-targets -- -D warnings`** clean (three
lints were introduced by the new file and fixed: two `chunks_exact_to_as_chunks`, one `print_literal`).

### Wall clock, every figure measured

| what | wall clock |
|---|---|
| `cargo build --release -p oracle-replay --bin replay_runner` | 2.74 s (10:06:07Z → 10:06:10Z) |
| control run, chain-186 pin, `ojz_fixture` | 4 s (10:06:18Z → 10:06:22Z) |
| candidate run, chain-189, `ojz_fixture` | 4 s (10:06:28Z → 10:06:32Z) |
| candidate run, chain-189, `ojz_slide_fixture` | 6 s (10:06:44Z → 10:06:50Z) |
| `cargo test -p oracle-replay --test aeon_pin` | 0.06 s in-test |
| **`./tools/replay_playthroughs.sh`** (the named runner, all 16 tests incl. 3 playthroughs) | **9 s** (10:20:17Z → 10:20:26Z), 8.29 s in-test |
| the same, re-run after the pin-naming step was added | **8 s** (10:34:36Z → 10:34:44Z), 8.34 s in-test |
| `cargo test --workspace` (debug, full) | **10 min 33 s** (10:21:13Z → 10:31:46Z, log mtime), backgrounded |
| `cargo clippy --all-targets -- -D warnings` (warm) | 0.17 s (10:34:53Z) |

The ~183 s debug figure for the three playthroughs is the module docs' own measurement, carried
forward; the release figure above is measured here.

---

## Enumeration: everything that touches the pinned data

Run and shown, per the bar that a completeness claim is cheap and the sweep is not. Searched for the
directory path, the env var, the resolver helpers, **and every artifact filename** — not just the
literal `fixtures/aeon`:

```sh
grep -rn -E "fixtures/aeon|ORACLE_AEON_DIR|aeon_dir|s4\.debug\.bin|s4\.debug\.lst|demo\.debug\.lst|demo\.lst|s4\.bin|s4\.lst" \
  --include='*.rs' --include='*.toml' --include='*.sh' --include='*.yml' --include='*.yaml' \
  --include='*.json' --include='*.py' . | grep -v '^\./target/'
```

**Two consumers read the frozen bytes**, both through their own `aeon_dir()` honouring
`ORACLE_AEON_DIR`:

* `crates/oracle-replay/tests/replay_real_artifacts.rs`
* `crates/oracle-core/tests/symbols_real_lst.rs`

…plus the two files added here (`aeon_pin.rs`, which reads the frozen directory *directly* and
deliberately not the override; `tools/aeon_pin_report.py`, which reads `PIN.tsv`).

> **✅ CLOSED 2026-08-30 — `docs/2026-08-30-live-tree-readers.md`, branch `parcel/live-tree-readers`
> (`254ab9e`, `7bb7331`, `ee72d16`, `2f5d99b`).** The finding below is kept as written, because the
> caveat it ends on earned itself. **Its example list was short**: four examples read the live tree,
> not the two named — `diag_soundqueue.rs` and `synth_render.rs` as well — and *"what this one grep
> surfaced"* is exactly why saying so was right. **And the count was not the only stale pin**: the
> same three lines also held `romBytes == 696836` and `Player_1 == 0x00FF8CFA / 0xFFFF8CFA`, and all
> four were wrong, measured live. The outcome is deliberately **mixed**: `s4.bin` repoints to the
> frozen copy, while `s4.soundtest.bin` (absent from sigil's goldens — nothing to freeze it from) and
> `demo.bin` (freezable, declined) keep a live default that now announces itself at startup.

**A finding the sweep turned up that is NOT this pin, and is left open:** `tools/aether_smoke.py` reads
`/home/volence/sonic_hacks/aeon/s4.debug.lst` and `s4.lst` **from aeon's live working tree**, and pins
`symbolCount == 2129` against them (line 84). That is a live-tree dependency of exactly the kind
`fixtures/aeon/` exists to remove, and the pinned count will drift whenever aeon rebuilds. Several
`crates/oracle-core/examples/*` do the same (`k4_openbus_probe.rs`, `vgm_capture.rs`). Out of scope
here; registered so it is not rediscovered as a surprise. **Not claimed as a complete list of live-tree
dependencies** — it is what this one grep surfaced.

---

## Open, and what is tagged

* **TAGGED for foreground runtime follow-up:** nothing in this parcel used the emulator MCP tools
  (they deadlock from background agents). Everything above is `cargo` and `replay_runner`, both of
  which run headless. No item here is waiting on a live-runtime confirmation.
* **NARROWED, NOT CLOSED (open, by construction):** the listings' assembler is 19 commits ahead of
  chain 189's `sigil_rev` and was dirty. The uncommitted portion is unrecoverable. See above and
  `PROVENANCE.md`.
* **Reported disagreement:** the pipeline-reachable file count in the relayed wording is three; I make
  it four (`test_support.rs`). Conclusion unaffected; enumeration corrected in `PROVENANCE.md`.
* **Not addressed here:** the live-tree dependencies in `tools/aether_smoke.py` and
  `crates/oracle-core/examples/*`. **✅ Addressed 2026-08-30 in `parcel/live-tree-readers` —
  `docs/2026-08-30-live-tree-readers.md`.** The enumeration there is wider than the one above; see
  the closure note beside the finding.
* **An ops note:** I ran `git fetch --quiet origin` once in the sibling `sigil` checkout at the start
  of this parcel, before adopting the rule that the reporter must not fetch on its own. sigil's
  `origin/master` moved twice more during the parcel (`3ad7ed02` → `05c81698` → `62691b84`) from other
  sessions, not from me. Chain number and both golden blobs were unchanged across all three, which is
  why every conclusion above still holds — but it is exactly the hazard the reporter's "your local
  mirror can be behind" warning exists for, observed live.
