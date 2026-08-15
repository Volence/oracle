# Handoff — where this left off, and what to pick up (2026-08-14)

Written at a session boundary. Everything described here is **committed and pushed**; both trees are
clean. Nothing is in flight, nothing is half-applied, and there is no working copy to rescue.

| repo | tip | state |
|---|---|---|
| `oracle-next` | `02cdba1` on `m68000-microop-framework` | pushed, clean |
| `empyrean` | `0326bb0` on `main` | pushed, clean |

Gates on the merged tree, run firsthand: `cargo test --workspace` **1158 passed / 0 failed / 24 legs**,
`cargo clippy --all-targets --workspace` zero warnings, `cargo fmt --all --check` clean, and
`crates/oracle-core/tests/` shows a **zero-line diff** — the currency files were not touched at all, so
no pinned literal *can* have moved.

---

## What just landed

Fable's round-2 rulings (`docs/2026-08-14-fable-rulings-round2.md`), all three queued items:

1. **All four `require_paused` wirings pinned** (`d0091a9`) — one table-driven wire test. The number
   worth remembering: **before this, deleting any of three of the four `require_paused(...)?` call sites
   left the whole suite green.** That is what "one tested refusal path out of four" actually cost.
2. **Phantom `D13 rule N` citations removed** (`a2626eb`) — **the count was five, not three.** Rules 5
   and 6 in `checkpoints.rs` were the same draft-schema drift as the three "rule 4"s. D13 has exactly
   three rules; the norms being cited are §6.1 sentences.
3. **`F-TRACE-EXPOSE-LATCHES`** (`62f067d`) — accessors shipped; **the authorized deletion was measured
   and refuted.** A sink never holds the machine, so the only call site is the frame boundary, but the
   five stateful counters gate on the latch *at each event*. Frame-boundary sampling moved **22 of 29
   sweep rows**. `K4Probe`'s write-stream shadow is the correct implementation, not duplication. Honest
   yield: accessors only, zero deletions. The register row and the superseded prose (`02cdba1`) both
   carry the retraction.

Contract amendments in `empyrean` (`0326bb0`): drop-is-idempotent, the cursor invariant (behavioral —
the token stays opaque), and the run-control rule widened to name `press`/`reload_rom`.

---

## ★ Next up

**Fable's ruling-F five-item priority list is fully discharged** (`docs/2026-08-14-fable-rulings.md:270-320`):
items 1–4 shipped, item 5 deferred on purpose. The queue below therefore comes from the recon's own
sequencing (`docs/2026-08-14-tooling-frontier-recon.md` §7), not from invention.

### 1. P3 — deterministic scripted input + the headless replay runner *(recommended first)*

The next unshipped phase, and the recon calls it **"first real payback to the engine"**: everything
shipped so far (P0/P1/P2/P5, the trace recorder, the latch accessors) makes *our* work better; this
makes *Aeon's* work better.

Fable pre-ranked it against everything we just built:

> *If the Aeon replay-runner work starts before this list is done, items 2–4 should yield to it — a
> working CI gate for the engine outranks instrument polish.*

The list is done, so nothing outranks it now. Leverage is unusually high because the hard half already
exists: **Aeon's replay desync net is built but dead, waiting only on deterministic input from us.**

### 2. The `cursor` type bug

Verified, pre-existing, unfixed. `contract/schema/bus-protocol.schema.json:330` types `cursor` as
`"string"`; `crates/oracle-aether/src/engine.rs:1265-1289` emits `json!(next_cursor)` built from
`as_u64()` — a JSON **number**. The §6.1 paragraph that just shipped makes *opacity* normative ("a
client MUST NOT parse it"), and a numeric cursor invites exactly the id-arithmetic the invariant exists
to prevent. **The schema is the correct side; the server is wrong.** Small fix (stringify on emit,
accept both on parse) but it changes a wire field, so it wants its own slice.

### 3. `F-TRACE-VDPWRITE-MCLK`

Fable: *"schedule it after 1–4, ahead of any further aggregation work."* Now unblocked. Giving
`VdpWrite` a per-write mclk makes sub-scanline CRAM effects locatable instead of hand-arithmetic, which
unblocks `F-CRAMDOT` — a blocker two prior arcs already ran into. Anchors: the `vdp.rs` capture path,
`system.rs:728-732`, register row at `docs/2026-08-14-trace-recorder-design.md:1064`.

### 4. The seven remaining magic frame budgets

P0 leftover; two of nine were converted as proof. **Each needs its own measurement — a blanket idle
detector is wrong.** `m68k_bcd` touches the VDP on frames 0, 6 and 530 and nowhere in between, so a
"the screen went quiet" predicate stops 523 frames before its answer exists.

---

## Owner-owed, not code, and longest-outstanding

**Nobody has plugged in a gamepad.** Track 1 usability — pad support, save states, reset/ROM-reload,
volume/mute, atomic `.srm` — shipped 2026-08-14 with 960 tests green and zero currency movement, and no
human has pressed a key since. Tests cannot answer whether the deadzone is sane, whether the save-state
hotkeys are where hands expect them, or whether any of it feels right. Only the owner can close this.

```
cargo run --release -p oracle-frontend -- <rom>
```

## Parked questions for the owner

From the recon's own §7 close:

- **(a)** Does the `deb2` decoder earn its cost now, or is the `.lst` path sufficient until symbols rot
  again?
- **(b)** Raise the cross-repo ask now or carry it — ship `s4.build.json`.
  > **CORRECTION 2026-08-15: CR-6 is RESOLVED and this entry was wrong to carry it.** Verified in the
  > contract itself: `protocol.md:253` makes camelCase normative (*"Event names are camelCase
  > (`romReloaded`, never `rom_reloaded`)"*), §10 decision 4 (`:682-685`) rules this contract wins and
  > forbids both-spellings bridging, `:718` records the CR as closed, the schema agrees
  > (`bus-protocol.schema.json:150,166`), and Aurora's spec was corrected (`aurora` commit `26378c9`).
  > Every surviving `rom_reloaded` in `protocol.md` is prose *explaining* the ruling. Nothing to raise.
- **(c)** How much of the 53-method catalog do we commit to, given the critique's finding that ~20
  well-shaped tools cover more ground?

One more, newly created by this arc: the `press`/`reload_rom` run-control widening is a **normative
scope change**, not a transcription, and it is now public. Reversing it is one contiguous edit at
`protocol.md:366-368` plus one §11 sentence — as a new commit, not an unpush.

## Keep dead

Stated plainly so momentum does not re-fund them:

- `F-TRACE-TUPLEKEY` — its one justifying episode keyed on VDP-internal probe results, not bus-event
  fields, so the evidence does not support building it *on this seam*.
- The `master` field on `BusEvent`.
- Min/max and percentile aggregation — no episode, ever.
- The bus-legality detector — called "highest strategic value" in a 2026-07-20 wishlist; two years of
  hunts have never needed it once.
- Interactive step/break, call stacks, and rewind, until something demands them.

## Two housekeeping notes

- `trial-merge` still exists locally at `26d9aef`, fully merged and three commits behind. Left in place
  because it is a staging convention, not garbage. Delete freely if it has outlived its use.
- 21 agent worktrees were removed and their branches deleted (all fully merged first, via `-d` never
  `-D`) — 24G reclaimed. `vendor/TestRoms` was verified intact afterward, since losing that symlink
  silently *skips* conformance rows rather than failing them.
