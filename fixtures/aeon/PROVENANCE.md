# `fixtures/aeon/` — our frozen copy of Aeon's build artifacts

These six files are **this repo's own committed copy** of the Aeon build outputs our end-to-end tests
read. They are checked in as bytes — not a fetch script, not a checksum manifest. A fetch would
reintroduce exactly the dependency this freeze removes.

## Why this directory exists

Two test files used to read Aeon's *live* build outputs out of a sibling checkout at
`/home/volence/sonic_hacks/aeon`:

| test file | reads |
|---|---|
| `crates/oracle-replay/tests/replay_real_artifacts.rs` | `s4.debug.bin`, `s4.bin`, `s4.debug.lst`, `s4.lst` |
| `crates/oracle-core/tests/symbols_real_lst.rs` | `s4.lst`, `s4.debug.lst`, `s4.bin`, `s4.debug.bin`, `demo.lst`, `demo.debug.lst` |

That made our suite's green depend on another team's lane not rebuilding their game. On 2026-08-29 at
22:35 Aeon rebuilt and four rows in the replay file went red — **foreign failures, not our bug**. The
replay fixture the runner replays is *embedded inside* Aeon's ROM, so an Aeon rebuild moves the recorded
stream out from under us, with nothing on our side to point at.

Freezing our own copy means the pin moves only when *we* decide, and every move is attributable.
Hub ruling: empyrean `27b58fc` — *"oracle freezes its own ROM copy, aeon_rev attribution only"*.

`ORACLE_AEON_DIR` still overrides the directory, so a developer can deliberately point the tests at a live
Aeon build. What changed is only the **default**: it is now this directory rather than Aeon's working tree.

## What is pinned

**sigil freeze `39c34fd2` — `replay-restamp-all-ten`, chain 189**, `aeon_rev 3f143178`.
All six artifacts sit at that one chain.

| file | bytes | chain | sigil freeze | `aeon_rev` | authority |
|---|---:|---:|---|---|---|
| `s4.bin` | 719,315 | 189 | `39c34fd2` | `3f143178` | sigil golden blob |
| `s4.debug.bin` | 736,315 | 189 | `39c34fd2` | `3f143178` | sigil golden blob |
| `s4.lst` | 280,720 | 189 | `39c34fd2` | `3f143178` | aeon build tree |
| `s4.debug.lst` | 330,541 | 189 | `39c34fd2` | `3f143178` | aeon build tree |
| `demo.lst` | 176,191 | 189 | `39c34fd2` | `3f143178` | aeon build tree |
| `demo.debug.lst` | 204,583 | 189 | `39c34fd2` | `3f143178` | aeon build tree |

**The record is kept per file, not per directory**, so that a partial move — one internally coherent
(ROM, listing) pair ahead of the rest — is expressible without a new format. It happens to be uniform
today; the arrangement does not depend on that.

**`PIN.tsv`, beside this file, is the machine-readable form of that table** — one row per artifact with
its sha256. Two things read it:

* `crates/oracle-replay/tests/aeon_pin.rs` — a default-suite gate that hashes every artifact against it
  and **prints the chain in the suite's output**, so a green can never be read as a statement about
  aeon's master. It asks the **recovery** question (*are our bytes our bytes?*) at the pinning revision,
  which is a fact about this repository alone.
* `tools/aeon_pin_report.py` — a **non-gating** reporter that asks the **currency** question (*has aeon
  moved past us?*) and therefore asks it at sigil's **tip**. Deliberately not a test: a gate that
  reddens because someone else moved puts the whole gradient behind bending our side until it passes,
  and it would reintroduce the sibling-checkout dependency this freeze exists to remove.

The chain number is **derived, not transcribed**: it is the count of `[[entry]]` blocks in that
revision's `crates/sigil-harness/golden/provenance.toml`. Verified at three revisions —
`5af70797` → 186, `39c34fd2` → 189, `origin/master` (`3ad7ed02` at the time of writing) → 189.

### The ROMs — from sigil's committed golden blobs

Taken as git blobs from sigil, never from a working tree (working trees move; sigil's goldens are frozen
and chain-attested):

```sh
git -C ../sigil show 39c34fd2:crates/sigil-harness/golden/s4.debug.bin
git -C ../sigil show 39c34fd2:crates/sigil-harness/golden/s4.bin
```

### The listings — from an Aeon build tree

The `.lst` listings are **not frozen upstream**. sigil's golden set contains the ROMs (`s4.bin`,
`s4.debug.bin`, `demo.bin`, `demo.debug.bin`, `config_a.bin`, `config_b.bin`, `lean.bin`) and **no
listings at all** — verify with:

```sh
# checked at the chain-186 freeze, the chain-189 freeze, AND sigil's tip — nothing at any of them:
git -C ../sigil ls-tree -r --name-only 3ad7ed02:crates/sigil-harness/golden/ | grep -i lst
```

So an Aeon build tree is the only source that exists. All four came from
`/home/volence/sonic_hacks/.aeon-chain189`, a detached Aeon worktree at chain 189's
`aeon_rev 3f14317886b343a6b94e4ac93ae06c7585e53ae5`, reported clean before the build and confirmed at
`3f143178` with an empty `git status --porcelain` when the bytes were taken. Built **2026-08-30
06:06–06:11**, copied here immediately afterwards.

> ⚠ **A listing's presence in an aeon tree is not evidence of which build produced it.** aeon's *live*
> working tree held an `s4.debug.lst` of **330588** bytes while chain 189's is **330541** — they had
> rebuilt in between, and taking the file on the strength of its path would have frozen the wrong one.
> Take listings from a tree whose HEAD you have checked, and prove the joint below.

### The consistency joint, and how it was checked

A listing is only valid for a ROM built from the same source. The ROMs come from sigil and the listings
from an Aeon tree, so that pairing has to be *proved*, not assumed. It was, three ways.

**1. The identity control — all four ROMs, not just the two we froze.** The build tree's own on-disk
ROMs were re-hashed and compared to sigil's committed golden blobs at `39c34fd2`:

```
05c2738d…62169c  .aeon-chain189/s4.bin         ==  golden 39c34fd2 s4.bin
4ee7ac79…a9a0b3  .aeon-chain189/s4.debug.bin   ==  golden 39c34fd2 s4.debug.bin
426d43ed…c883c0  .aeon-chain189/demo.bin       ==  golden 39c34fd2 demo.bin
938cb954…7c2a012 .aeon-chain189/demo.debug.bin ==  golden 39c34fd2 demo.debug.bin
```

Four for four. So the build that produced these listings produced chain 189's ROMs exactly.

**2. An independently-taken copy agrees.** The `s4.debug.lst` here is **byte-identical**
(`81a11102…845a2f`) to the one in `/home/volence/sonic_hacks/restamp-ab-chain189/`, a read-only
snapshot with its own `SHA256SUMS`, captured at 05:13 by the lane that ran the chain-189 A/B
(`docs/2026-08-30-restamp-ab-chain189.md`) — an hour before this build.

**3. The runner's own binding check**, against sigil's committed blob rather than the tree's ROM:

```
$ ./target/release/replay_runner --rom <39c34fd2 golden s4.debug.bin> \
      --lst .aeon-chain189/s4.debug.lst --fixture ojz_fixture
  lst: 2743 symbols, bound to this ROM (deb2 appendix at $0A7F40, 48379 bytes)
  PASS — the stream ran to its end, corroborated three ways.
```

`$0A7F40` is corroborated independently: sigil's own `provenance.toml` records `anchor_end = 0xa7f40`
for the chain-189 `s4_debug` target. And `48379 = 736315 − 0xA7F40`, so the appendix spans exactly the
tail of the image the listing claims.

The suite re-proves the joint on every run:
`real_shape_binding_accepts_the_matching_rom_and_refuses_the_crosses` binds each listing to its ROM
through the `deb2` appendix and refuses both crosses, and `the_wrong_shape_listing_is_refused` checks the
release/debug cross from the replay side.

### ⚠ NARROWED, NOT CLOSED — the assembler that built these listings is not chain 189's

Flagged by aeon unprompted, and it must not be softened. Chain 189 records
`sigil_rev = 5552bccbcb1938d4212c2dca41151cf617ca51ac`. The build above reported
`sigil --version` = **`d5967f87-dirty`**. The four byte-identical ROMs say the two assemblers agree on
everything that reaches a ROM — **but a listing is not a ROM**, and no ROM comparison can speak for the
listing emitter.

**The statement of record.** Listings reproduce chain 189's four ROMs exactly and are byte-identical to
an independently-taken snapshot. The assembler was **19 commits ahead** of chain 189's recorded
`sigil_rev` and self-reported **dirty**. Read at the **file level rather than by path**, that range's
code changes are: a new write precondition and its single call site in the sound emitter module
(`seam2`); publication of the `--aeon` argument as `AEON_DIR` in the two argv callers (`sigil-cli`,
`emit_sound_blob`); and the hoist of that precondition's fallback path into a named constant
(`test_support::LIVE_TREE_FALLBACK`) so the resolver and the refusal cannot name different paths. Every
one of them governs **write destination and refusal**; none alters emitted content, and nothing in the
range touches listing or symbol emission — `sigil build`, which emits the listing, changes by 13 lines of
environment publication only. The uncommitted portion of the dirty tree is unrecoverable.
**Narrowed, not closed.**

The supporting checks, run here rather than accepted:

* **A listing cannot self-identify its toolchain**, which is why this has to be written down rather than
  recovered later. `s4.debug.lst` carries no assembler version marker at all: every `dirty` occurrence in
  it (34 of them) is a game symbol — `Enqueue_Dirty_Buffers`, `Palette_Dirty`, `not_dirty` — and the only
  `VERSION` rows are the game's own `ART_HDR_VERSION` and `HW_VERSION` equates.
* **`5552bccb` is an ancestor of `d5967f87`, 19 commits.** The range touches 13 files: 5 docs, and 8
  code/data. Classified by **what they do**, not by what they are named:
  | file | role |
  |---|---|
  | `crates/sigil-harness/src/seam2.rs` | **pipeline — this IS the sound emitter module.** `emit_sound_blob.rs` is a thin argv wrapper over `seam2::emit_{dac,mt,sfx,seq_opcode,sound_tables,pitchtable}_artifacts` — six call sites, verified at `d5967f87` lines 70/76/82/88/93/99 |
  | `crates/sigil-cli/src/main.rs` | pipeline — the build CLI that emits the listing |
  | `crates/sigil-harness/src/bin/emit_sound_blob.rs` | pipeline — invoked by aeon's `build.sh:372`; its blob goes into the ROM |
  | `crates/sigil-harness/src/test_support.rs` | pipeline — holds `LIVE_TREE_FALLBACK`, which `seam2` resolves and names in its refusal |
  | `crates/sigil-harness/golden/provenance.toml` | data — the freeze ledger; the +21 lines *are* chain 189's own entry |
  | `crates/sigil-harness/tests/…` ×3 | tests — separate cargo targets, structurally unlinkable into either binary |
* **The emitter module's delta is demonstrably a refusal and nothing else.** Strip doc comments from
  `seam2.rs`'s +56 and **exactly two code additions remain**, verified line by line here:
  ```rust
  pub fn require_named_reference_tree(aeon: &Path) -> Result<(), String> { … }   // new: an AEON_DIR presence check
  require_named_reference_tree(aeon)?;                                           // its one call site
  ```
  The rest of the +56 is documentation. **It adds a refusal. It changes no emitted byte.** The two argv
  callers publish their `--aeon` argument as `AEON_DIR` so they satisfy the rule rather than being
  exceptions to it. Where a write may go, and when it is refused — not what is emitted.
* **The direction of the risk is inverted, and the safety was exercised rather than assumed** — aeon's
  measurement, credited to them and not re-derived here. Under chain 189's *own* assembler, an unset
  `AEON_DIR` fell back to the live aeon working checkout, whose `engine/sound/generated/` is gitignored,
  so a stray write would have left no trace. The newer binary refuses instead. During the build that
  produced our listings the precondition fired as intended: `engine/sound/generated/` in the live
  checkout is stamped 05:52:32 and untouched, while the same directory inside `.aeon-chain189` is
  stamped 06:11:11. On the single axis where this assembler differs from chain 189's, it is the **safer**
  one. Our conclusion rests on the diffs regardless.
* **Irreducible residue:** `-dirty` means uncommitted changes at build time, which no revision can
  recover. sigil's tree carries one dirty test-harness file *now*, but that is a reading taken later and
  bounds nothing about what was in the tree at 06:06. aeon additionally disclosed that the two binaries
  used are not from the same build moment (`emit_sound_blob` mtime 04:06:26, `sigil` 05:40:26) and can
  name a revision for neither. What covers that is not provenance but the artifact: all four ROMs
  reproduce chain 189's committed goldens byte-for-byte, and the sound blob is *inside* those ROMs.

> ⚠ **What does NOT narrow it, though it looks like it should:** the byte-identity between these
> listings and the earlier snapshot. That snapshot is itself a copy of a listing *aeon* built, so the
> comparison is aeon's build against aeon's build — if both ran the same non-chain-189 assembler they
> agree and prove only reproducibility. It is a real and useful control against taking the *wrong file*;
> it is not a control on the *toolchain*. The committed-diff check is what narrows the toolchain
> question, because it enumerates over a different parameter entirely.

> ⚠ **And a note on how that delta was enumerated, because it went wrong three times before it went
> right, and the corrections kept landing on the file that mattered most.** The first pass filed
> everything under `crates/sigil-harness/` as "harness, therefore tests" — a classification by **path
> prefix**, i.e. by what the code is *named*. `emit_sound_blob.rs` is named harness and is invoked by
> aeon's build. The second pass caught that one and still filed `src/test_support.rs` as tests on the
> same reasoning; it is reached from `seam2.rs`. The third pass caught *that* and still described
> `seam2.rs` as merely "their shared write precondition" — when `seam2` **is** the sound emitter module,
> the most load-bearing file in the range, originally misfiled as test code. **Each correction came from
> classifying by role instead of by name, and each one moved a file toward the pipeline, never away from
> it.** The `tests/…` three are excluded on a structural argument — a cargo integration-test target
> cannot be linked into a binary — not on their directory.

## The repair, and what it actually was

### A PIN MOVE, not a payload patch

Our chain-186 `s4.debug.bin` was **stale**: nine embedded checkpoint payloads at `$0A6CDC…$0A6D33`
disagreed with what the machine produces, and `the_standing_fixture_runs_green` failed on it at
`Logic_Tick 1154` (measured, reproduced without any instrument —
`docs/2026-08-30-restamp-ab-chain189.md`).

The nine repair payloads were known byte-for-byte. **We did not apply them.** Patching our frozen
chain-186 ROM in place would have produced an artifact belonging to *no chain*, and attributability is
the entire reason this directory exists. The pin **moved to a chain-attested set** instead. The result
is byte-for-byte chain 189's published `s4.debug.bin`, because that is what chain 189 *is* — a pure
fixture re-record over chain 188, no code bytes moved.

### Chain 189 is sigil's tip chain, not merely a newer one

Checked at tip rather than at the pinning revision, which is the only place a currency question can be
asked: sigil `origin/master` carries **189** `[[entry]]` blocks, its last entry is
`replay-restamp-all-ten` at `aeon_rev 3f143178`, and its golden `s4.debug.bin` / `s4.bin` blobs are
**byte-identical** to `39c34fd2`'s — 26 commits of sigil movement, zero golden movement.

### Compare hashes, never lengths

Chain 188 → 189 is **not** byte-identical while both `s4.debug.bin` images are exactly **736,315**
bytes; the hop before it (187 → 188) *was* byte-identical, which is exactly what trains a reader to skim
the next one. `s4.bin` went 719,235 → 719,315 and `s4.debug.bin` 736,095 → 736,315 in this move, but a
matching length has already passed a different ROM here once. Every check in this file is a hash.

### ⚠ `s4.bin` DOES carry a fixture stream — settled from the bytes, because the convenient answer was false

Settled from the bytes, not assumed. The release image carries the replay stream; what it lacks is the
*compare*. `replay.emp:174-186` gates the checkpoint compare on `DEBUG == 1`, so the release shape steps
over each payload without comparing it — which is why `policy::require_debug_rom` refuses a release ROM
outright, and why the release image contains no `REPLAY DESYNC` string.

Byte-searched directly, all nine checkpoint payloads, in all four images:

| image | nine STALE payloads | nine REPAIRED payloads | `REPLAY DESYNC` |
|---|---|---|---|
| chain-186 `s4.bin` (ours) | **present**, `$0A4A2C…$0A4A80` | absent | no |
| chain-186 `s4.debug.bin` | present, `$0A6CDC…$0A6D30` | absent | yes |
| chain-189 `s4.bin` | absent | **present**, same offsets | no |
| chain-189 `s4.debug.bin` (ours) | absent | present, `$0A6CDC…$0A6D30` | yes |

**Why this mattered while the move was being decided.** A pairwise move — taking only the `s4.debug`
pair and leaving the release shape behind — was on the table, and *"the release build carries no
fixture, so nothing stale is left behind"* would have been the convenient answer that made it free. It
is false: the release image carries the whole stream, nine stale payloads included. Since the full
re-pin landed, `s4.bin` is chain 189 too and the question is moot — recorded so the next partial move is
argued against the bytes rather than against the assumption.

Corroborated independently from sigil's own chain-189 entry, which notes the `s4` **release** golden
moved in the restamp while `demo` and `demo.debug` did not: *"the demo shapes carry no fixture, which is
why they must not move."*

The two `demo` listings have no ROM counterpart here — the only test that reads them
(`real_demo_pair_documents_the_binding_checks_residual_limit`) compares the two listings against *each
other* and loads no demo ROM. They are frozen from the same build run.

## Build identity — `aeon_rev 3f143178`, attribution only

```
aeon_rev = "3f14317886b343a6b94e4ac93ae06c7585e53ae5"
```

Read out of sigil's `crates/sigil-harness/golden/provenance.toml` at `39c34fd2`, tip chain entry
(chain 189, `replay-restamp-all-ten`) — and still the tip entry at sigil's `origin/master`.

**This is attribution, not a dependency.** Nothing in this repo reads `aeon_rev`, resolves it, or moves
because it changed. It is recorded so a reader of these bytes can say which build they are looking at.

## ⚠ HISTORICAL — why chain 186 and not the newer chain 187

> **Superseded 2026-08-30 by the move to chain 189.** Kept in full, per this repo's supersession rule:
> it records the reasoning that made 186 the right pin *at the time*, and the ruling it ends on
> (*"KEEP PINNING 186"*) is the one this move reverses, so it must remain readable next to the reversal
> rather than be quietly deleted. The condition that ruling named — *"aeon will signal when a ROM exists
> whose fixture is coherent"* — was met: chain 189 is that ROM.

The parcel that created this directory was briefed to freeze sigil's tip at the time, `dd371e3b` —
`freeze: scroll-and-section-clamps`, chain 187, `aeon_rev ec6a4791`. It does not, and the reason is
measured, not argued:

* Chain 187 is **FROZEN-BUT-UNATTESTED**. Aeon's post-freeze strict suite went **red — 8 failures, 7 of
  them one cross-seam symbol** — and a **superseding freeze is expected** once sigil's fixes land. Aeon
  expects the ROM bytes to be identical across that superseding freeze but **would not promise it**.
* Chain 187's `s4.debug.bin` is **byte-identical to the Aeon build that broke our four replay rows**
  (sha256 `951cf960…62707d`, Aeon's on-disk ROM at 22:35). Freezing it would have frozen the breakage:
  measured directly, `ORACLE_AEON_DIR=<chain-187 copy> cargo test -p oracle-replay --test
  replay_real_artifacts` gives **9 passed / 4 failed**, the same four rows, because the ROM's embedded
  replay fixture disagrees with the ROM's own game code. There is no stale constant on our side to fix:
  the game itself raises `REPLAY DESYNC` at ring 0 (recorded `490164326`, produced `221728870`).
* Chain 186 is the last freeze whose embedded fixture is coherent: the same command against it gives
  **13 passed / 0 failed**, with our code unchanged.

Adopting a known-red upstream build as our own regression baseline would give us four permanently red
rows that signal nothing. So this pin is the last coherent freeze, taken by the same recipe and verified
to the same standard (ROM bytes equal to a committed sigil golden; listings from a clean tree at that
freeze's own `aeon_rev`).

**Open question, deliberately left visible:** the chain-187 desync is either Aeon's — a replay fixture not
re-recorded after the scroll/section-clamp change — or **ours**, an emulator inaccuracy that the new
clamp code exposes. This freeze does not settle that, and pinning 186 must not be allowed to bury it.
Reproduce the failing case at any time with:

---

### ✅ ANSWERED 2026-08-30 — IT IS AEON'S, AND OUR EMULATOR IS NOT IMPLICATED

*Original question kept above per this repo's supersession rule; the answer sits over it, not in place of it.*

**Verdict: (a). The fixture is stale. (b) — an emulator inaccuracy on our side — is dead.**
Answered by the aeon lane, booked at aeon `0b612953` (*"book: the replay fixture needs a
prove-then-restamp after the clamps"*, `docs/DEFERRED_WORK.md` +45; verified here as reachable at
their `origin/master`, and a docs SHA carrying a docs booking, which is the right class for what it
anchors).

**The mechanism, which is a line of their source rather than anyone's prior** — all four checks below
re-run firsthand in their tree at their tip, not transcribed:

* `engine/system/replay.emp:374` is `dc.l Section_Right_Col_Written, 1 // Right + Left` — a **hashed
  cell**, sitting in the fold table the checkpoint net is built from.
* Their section clamp is exactly what moves that word: `Section_RedrawPlanes` used to *assign*
  `d7 = Cache_Head_Col` and now clamps to `min(start_world_col + 63, Cache_Head_Col)`. The two differ
  precisely when `cam_col < 16`.
* **The act opens at camX 96 — `cam_col` 6 — which is inside that window.** That is why the desync
  fires at ring 0 rather than partway into the run, and the ring-0 position is what makes the
  explanation load-bearing rather than merely available.
* **The parallax half cannot contribute, checkably**: the fold's own header (`replay.emp:277`) reads
  *"Excludes sound RAM, `Ctrl_*` cells, VDP staging, DEBUG-only cells — gameplay state"*, and the module
  matches `Vscroll`/`Parallax` **zero** times. A V-scroll clamp is VDP staging and is invisible to the
  net by construction.

**⚠ CONSEQUENCE FOR THIS PIN, AND IT IS THE OPPOSITE OF WHAT YOU WOULD ASSUME: KEEP PINNING 186.**
*(Superseded 2026-08-30 — the signal it waits for arrived; the pin is chain 189. Its reasoning held
exactly as written: chain 188's ROM was still stale, so nothing before 189 would have helped, and the
"blanket re-stamp" warning below is why the repair was taken as a pin move to a chain-attested set
rather than as nine bytes patched into our own copy. Read on for what it got right.)*
The superseding freeze now running does **not** re-record the fixture — re-recording was not part of
sigil's eight-failure fix and not part of aeon's freeze; it was an unbooked job until this question was
asked. **So a supersede's ROM will still desync us.** Do not pin a newer freeze on the assumption that
superseding fixed this. aeon will signal when a ROM exists whose fixture is coherent.

**And do not expect a blanket re-stamp to be the fix.** Their ritual is **prove-then-restamp** under an
owner ruling (their d-14): prove the clamps are the only behavioural change the net sees, then re-stamp
only the checkpoints that legitimately moved. The fold is deliberately address-free so a behaviour-neutral
parcel reproduces recorded hashes — *a desync therefore means real behaviour moved*, and a blanket
re-stamp would restore green while destroying the only claim the net makes.

Reproduce the historical failing case at any time with:

```sh
ORACLE_AEON_DIR=/home/volence/sonic_hacks/aeon cargo test -p oracle-replay --test replay_real_artifacts
```

## The bytes, as committed

sha256 of every artifact in this directory as committed. A later reader can check the bytes without
trusting any of the story above.

| file | bytes | sha256 |
|---|---:|---|
| `s4.bin` | 719,315 | `05c2738d759a119dde2d7c799b51fceff2e2c07a89a4cd2cf87adae60e62169c` |
| `s4.debug.bin` | 736,315 | `4ee7ac79737f1decc16c13cef4e160ed26c3fea078b3f5b2b7c4300857a9a0b3` |
| `s4.lst` | 280,720 | `b3e1dc424c209a643761dcf4133a9bbf7b18d602e8cdaa51a86db2517e9a48fe` |
| `s4.debug.lst` | 330,541 | `81a111020e3f28ddda374648e7a3e1425cbde00ce5b09ea5769b83eb79845a2f` |
| `demo.lst` | 176,191 | `46059a06b963ed00f4350f10eaf6ccc3ca81012a82c67c8808363f1c73d14fbb` |
| `demo.debug.lst` | 204,583 | `6e28e3014ca1c563b7177d05718181ebc588d44b854d48d1e688c3c7eae62cdb` |

Reproduce with:

```sh
sha256sum fixtures/aeon/*.bin fixtures/aeon/*.lst
```

The same figures live in `PIN.tsv`, where the suite reads them:
`cargo test -p oracle-replay --test aeon_pin -- --nocapture` re-hashes all six and prints the chain.

**The previous pin's bytes, for anyone tracing the move** (chain 186, `5af70797`, `aeon_rev def98ee5`):
`s4.bin` 719,235 `b0873bed…be3351` · `s4.debug.bin` 736,095 `75e9f4d4…1fcf7a` · `s4.lst` 280,300
`98cc5b60…2c9b8b6` · `s4.debug.lst` 329,345 `d478dec2…4feccb` · `demo.lst` 175,771 `7f4c41fe…3db88d` ·
`demo.debug.lst` 203,387 `2c4ffa8e…daedb60`.

## Moving the pin — deliberately, never silently

**The rule: this pin never moves to make a red test go green.** A number re-pinned to "whatever Aeon last
built" is a pin that cannot fail and therefore detects nothing. If these tests go red, the first question
is *what changed and is it ours* — never *what value would make it pass*.

Move the pin only when we have decided to adopt a newer Aeon build — typically once a superseding sigil
freeze lands **and is attested**. The steps:

1. Pick the sigil freeze commit to adopt. Read its `crates/sigil-harness/golden/provenance.toml` tip entry
   for `aeon_rev`, the freeze name and chain number, and whether it is attested.
2. Take the two ROMs out of that commit as git blobs
   (`git -C ../sigil show <rev>:crates/sigil-harness/golden/s4.bin`, and `s4.debug.bin`) — not out of a
   working tree.
3. Take the four listings from an Aeon tree that built that exact freeze. Prefer a **clean** tree whose
   `HEAD` equals the freeze's `aeon_rev`, and record which tree it was.
4. **Re-verify the joint immediately**: re-hash that tree's on-disk ROMs — **all four**, not only the two
   you are freezing; `demo.bin` and `demo.debug.bin` cost nothing and widen the control — and confirm they
   equal the golden blobs you just took. If any differ you have a mismatched ROM/listing pair — **stop**.
   Do not resolve it by taking the ROM from the working tree to match the listing.
   *A file's presence in a tree is not evidence of which build produced it: check `HEAD` and
   `git status --porcelain`, compare sizes and hashes against the snapshot you expect, and never take a
   listing from a live working tree on the strength of its path.*
5. **Re-run the full playthroughs, not just the default suite**: `./tools/replay_playthroughs.sh`. This
   is the step that would have caught the chain-186 pin going stale, and it did not exist then. The tick
   counts in `crates/oracle-replay/tests/replay_real_artifacts.rs` (`Fixture::Ojz` 1721,
   `Fixture::OjzSlide` 2350) are ROM-derived — the replay fixture is embedded in the ROM — so if the new
   ROM yields different values, re-derive them **once** and name the cause in the commit message. If they
   are unchanged, say so explicitly rather than letting a reader assume you checked.
6. Update **both** records: this file (every sha256, the sigil revision, the `aeon_rev`, the capture
   time, the source tree, the status notes) **and `PIN.tsv`**, which is what the suite actually reads.
   `cargo test -p oracle-replay --test aeon_pin` fails until they agree, so this step cannot be skipped
   silently — but it can be skipped *loudly*, and the fix is never to loosen that test.
7. Commit the artifact bytes, this file, `PIN.tsv`, and any tick-count change **together**, with a
   message naming which build you moved to and why.
8. Run `python3 tools/aeon_pin_report.py` afterwards and read it. It will not fail; it will tell you
   whether the pin you just set is current with sigil's tip, and which rows it could not measure at all.

## Frozen history

| when | sigil freeze | `aeon_rev` | listings from | note |
|---|---|---|---|---|
| 2026-08-30 03:55Z | `5af70797` — `fg-left-edge-borrow`, chain 186 | `def98ee5` | `.aeon-ref-186` (clean, at `def98ee5`) | initial freeze — ends the live-tree dependency on Aeon. Chain 187 (`dd371e3b`) was the briefed tip but is unattested and desyncs the replay fixture; see above. |
| 2026-08-30 10:xxZ | `39c34fd2` — `replay-restamp-all-ten`, chain 189 | `3f143178` | `.aeon-chain189` (clean, at `3f143178`, built 06:06–06:11) | **the pin was STALE and it was the whole reason it moved.** Chain 186's `s4.debug.bin` desynced at `Logic_Tick 1154`, checkpoints 18–26 of 27, and the one test that says so was `#[ignore]`d. Chain 189 is a pure fixture re-record (no code bytes) and is sigil's tip chain. Not a payload patch: the pin moved to a chain-attested set. Identity control: all four ROMs byte-identical to sigil's goldens. Tick counts 1721 / 2350 **unchanged** — checked, not assumed. Caveat: the listings' assembler was 19 commits ahead of `sigil_rev` and dirty; see *"NARROWED, NOT CLOSED"*. |
