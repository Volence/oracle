# Handoff — the conformance queue closed, and what enforcing a rule turned up (2026-08-15, later)

Follows `docs/2026-08-15-handoff-capability-layer.md`, whose §3 work queue this session executed. **Items 1
(all three) and 2 are done.** The headline is not the four items: it is that **enforcing** §8 item 19 found
four violations where the plan named one, and that a validator built for item 15 found, on its first run, a
divergence where the server **had no conformant option at all**.

## State

| repo | tip | state |
|---|---|---|
| `oracle-next` | `m68000-microop-framework` | committed, **not pushed** |
| `empyrean` | `90178fc` on `main` | committed, **not pushed** — two contract amendments |

Gates, run firsthand on the merged tree with nothing else contending:

- `cargo test --workspace` → **EXIT=0, 1360 passed / 0 failed / 4 ignored / 32 legs** (baseline 1330/30).
- `cargo clippy --all-targets --workspace` → 0 warnings; `--no-default-features` → 0 warnings.
- `cargo fmt --all --check` → clean.
- **`git diff 0a31d09 HEAD -- crates/oracle-core/tests/` is a zero-file diff.** Third session running.

---

## 1. The work queue, closed

**Item 1a — checkpoint ids are strings** (§8 item 16). Internal counter stays `u64`; `Checkpoint::wire_id()`
is the single mapping point. Five wire positions, not the four the brief named — `restore`'s `-32603`
decode-failure `data.id` was the fifth. `parse_checkpoint_id` is **strict** (string only, no numeric
fallback), deliberately unlike `parse_cursor`, which accepts both: a cursor is only ever round-tripped,
while an id is the handle a human hand-types, and typing `{"id": 3}` **is** the arithmetic D9 category 4
forbids.

**Item 1b — `reason:"runFrames"`** (§8 item 13). CR-1 was adopted a day earlier and never migrated.
`press` emitted the same wrong `"step"`; §3 rules `step` out affirmatively for a frame advance and the enum
is closed, so it now emits `runFrames` too, with **CR-9** raised for the residual ambiguity.

**Item 1c — the schema validator** (§8 item 15). Every line the test client receives is validated against a
vendored copy of `bus-protocol.schema.json`; `jsonschema` 0.49 as a dev-dependency only (`oracle-aether`'s
runtime deps are still `oracle-core` + `serde_json`).

**Item 2 — `emulator/pixel_attribution`** (§8 item 19). Contract first, handler second. Verified live:
`-32004` with `width`/`height` on both axes, the last valid dot answers, a blanked dot answers with
exactly one backdrop candidate, and the reply's key set is **exactly** the schematized one.

---

## 2. ★ What enforcing item 19 found: four violations, not one

The handoff named `pixel_attribution`. A sweep of the whole player found it ranks **third**:

| | capability | state |
|---|---|---|
| **largest** | watch hit log — hits (`seq/frame/pc/addr/old→value/via`), the ring's drop count, and VRAM/CRAM **range** watches | §6 has one row, `watchpoint_add → addr`. It can express none of them. **CR-11/CR-12 drafted.** |
| | SAT / sprite decode | no catalog row anywhere |
| | `sprite_tile_at` | not on the bus, not catalogued, **not even in `oracle-core`** — now moved there |
| | pixel attribution | the named one. **Closed.** |

**A live bug fell out of the watchpoint design and is fixed** (`a542b54`). `Watchpoints::clear()` retires
the specs and *keeps* recorded hits — deliberately, and documented — while the player clears-and-rearms on
every click and `dump_hits` printed no watch id. Two clicks produced one interleaved log with no way to
tell which pixel a hit belonged to. `WatchHit.watch` already carried the attribution; only the printing was
missing. This is the stale-watch contamination the negative record warns about, found in our own player.

**A latent fixture bug, found while writing the parity test.** `pick.rs`'s fixtures wrote reg `$0C = $81`
(H40) *before* reg `$01` (mode 5). The mode-4 register mask discards writes to registers above 10 while M5
is clear (`vdp.rs:758`, `:1605`), so both fixtures silently ran **H32** while their comments claimed H40.
Every assertion they made lives inside 256 px, so nothing they pinned was wrong — the comment was.
Reordered, with the cause recorded.

---

## 3. ★ Four wire divergences, and how each was found

| | divergence | found by | outcome |
|---|---|---|---|
| **CR-10** | no coordinate-shaped read; attribution is panel-only | sweeping the GUI against the catalog | **adopted**, contract `28ef4bb` |
| **CR-13** | ten methods emit result keys in no contract text | diffing live result key sets against §6 | **raised**, awaiting ruling |
| **CR-14** | `lookup_symbol.otherMatches` is an object; schema says array of strings | probe + validator, independently | **raised**, registered as a live divergence |
| **CR-15** | `$defs/id` forbids the `null` JSON-RPC 2.0 **mandates** | the validator, first run | **adopted**, contract `90178fc` |

**CR-15 is the one where the server had no legal move.** §2 adopts JSON-RPC 2.0 and §8 item 2 makes that a
conformance item; §5 of that standard *mandates* `"id": null` when the request id could not be detected.
Inventing an id attributes a failure to a call the client never made; omitting it breaks the standard
twice. The fix was made **narrower than the CR asked**: a bare nullable error id is wider than the standard,
because `-32700` and `-32600` are the only codes decided before a request object exists — on any other code
a real id was available to echo, so a null one is a correlation bug. The schema carries an `if`/`then`.

**CR-14 is the first type-level divergence where our shape looks like the better one**, so neither side moved
unilaterally: D14 says a disagreement is a spec bug, and the schema governs until amended. **The reference
server is non-conformant here today, knowingly.**

---

## 4. Three lessons that will recur

**A probe measures its own reach.** The first 33-message run reported exactly one failure. It called
`lookup_symbol` with a name that does not resolve, so it only ever drove the **error** path — CR-14 lives on
the success path. The in-tree validator has no such weakness: it rides every message the existing suite
already produces, which is how it found CR-15 in three tests that had been green for weeks. Treat any
"only N failures" figure from a hand-written probe as a **floor**.

**"Validated against the schema" ≠ "conformant."** D14 puts behaviour under the prose, and the split has
teeth: `reason: "step"` for a completed `run_frames` **passes** the schema, because `step` is a legal enum
member. Of the three conformance items in this arc the validator catches **one** and is blind to another by
construction. Item 13 has its own behavioural assertions for exactly this reason.

**A harness must report its own coverage or it reads as complete.** The schema is a SEED: 9 of the 21
advertised methods have a `result` schema. The harness prints the split, pins the uncovered list, prints
the registered divergences, and ends in the words *"this server is NOT fully schema-conformant."* Green now
means "no **unregistered** divergences", which is weaker and far more useful.

---

## 5. Two mechanisms that worked, and one that fired for real

**The divergence registry's anti-rot property is not theoretical.** CR-15 was ruled, the contract amended,
the copy re-vendored — and `every_registered_divergence_is_still_live` went **red**, because the registered
canonical message was no longer rejected. That failure is the only reason the stale entry was removed rather
than sitting quietly wrong. Built and exercised the same day.

**The un-framed Fable ruling earned its keep again.** It adopted CR-10 *with changes*, and the changes
mattered: it caught a same-minute CR number collision, corrected a normative sentence that was wrong in the
safe-looking direction (see §6), and struck **two false provenance claims** before they could enter the
contract's permanent amendment log.

**Contract-leads is now observable.** Between the two, the coverage report showed
`emulator/pixel_attribution` under *"schematized but not advertised"* — the row existing before the handler.

---

## 6. Claims corrected at source

- **"We shipped the panel one day after making the rule binding"** (`handoff-capability-layer` §2 item 8 and
  the design doc). False both ways: `pick.rs` landed **06:28**, item 19 landed **12:14–12:28** the same day.
  The panel **predates** the rule by ~6 hours. Corrected in both.
- **"Designed bus-first thirteen months ago."** `docs/2026-07-01-vdp-design.md` is **six weeks** old.
- **"Pause first, or read the stamp"** as the way to reconcile `screenshot` with attribution. Wrong:
  attribution is a whole-frame-state read *by construction*, and the overnight per-scanline work measured
  **6 of 17 ROMs diverging post-hoc vs live**, one making **zero** active-display writes. They disagree
  paused or not; the reconciliation path is the per-scanline capability. The corrected sentence is now
  normative in §6.
- **The handoff's ranked item 4 conflates two instruments.** A per-frame *sampler* never existed; a watch
  *recorder* does, with **two** executed consumers (the player, and `examples/watch_probe.rs`). That second
  consumer also disproves the sweep's *"this outranks attribution on evidence"* — they are peers at 2 each.
- **CR-9's own text quoted `port` as though §6 catalogued it.** It does not. That misquote is CR-13 in
  miniature and is annotated in place.

---

## 7. Next

1. **CR-11/CR-12 — the watchpoint surface.** Drafted with paste-ready fragments in
   `docs/2026-08-15-watchpoint-bus-surface.md`, adopt-both-or-neither, directed as next by the CR-10 ruling.
2. **CR-13 and CR-14 need owner rulings.** Both block writing the 12 missing schema fragments: writing them
   from what this server emits would encode the implementation as the contract.
3. **Queue item 3 — the MCP server** as a *client* of Aether (D10), untouched this session.
4. **Neither repo is pushed.** Two contract amendments are the outward-facing part; that is the owner's call.

## 8. Owner-owed, unchanged and now longer

- **Nobody has plugged in a gamepad.** Deadzone 0.5 is still an unfelt guess.
- **Nobody has heard the SY-7 mix levels.**
- Now joined by: **nobody has clicked the new watch-hit log** since it started printing watch ids.

## 9. Ops notes

- **Worktrees were stale for every agent — 6 for 6 — all cut at `24843e0`**, two commits behind the
  session's starting `HEAD`. The handoff's "cut from the session-start commit" is **wrong**: they are cut
  from a fixed older commit regardless of when the agent launches. `git worktree list` cannot detect this —
  it reports post-correction state. Only the agent's own base check catches it. Every brief must open with
  one, naming a commit message string and a file that must exist.
- A clean `git merge --ff-only` onto the required commit is the right recovery when the stale base is a
  strict ancestor and the tree is clean. Six agents did this; none lost work.
- **`cargo test --workspace` run concurrently from two trees corrupts the picture** — one run's leg count
  went *backwards* (18 → 3) with a second writing the same output file. Serialise the gate.
- `pkill -f "release/oracle-frontend"` still kills its own shell (exit 144). Kill by PID.
- Worktrees pruned **7.3 GB → 3.6 GB**; merged branches deleted with `-d`, never `-D`; `vendor` verified
  intact afterwards (17 `TestRoms` entries).
- A fresh worktree still needs `ln -s …/oracle-next/vendor vendor`.

## 10. Known and deliberately not fixed

- **`height` is 224 unconditionally** in `pixel_attribution`, from `Vdp::active_display`, which is
  pre-existing and NTSC-V28 by construction. The schema's description mentions 240 in V30; reporting 240
  would name a geometry nothing here renders.
- **`sprite.tile` absent-not-invented is unreachable through the bus** — attribution and `sprites_decoded`
  read the same live state inside one handler call, so there is no interval for the SAT to move in. The
  positive invariant is pinned instead, on a fixture that genuinely diverges the SAT cache.
- **`anyMessage` is a `oneOf` over both wire directions**, so an event that grew an `id` validates as a
  request. Written as a control that *should* fail; it passed. Now pinned as a test asserting the hole is
  **still open**, so it cannot close silently. Closing it needs the envelope split client→server /
  server→client — a contract change.
- **`rpc::bounded_array` emits `cursor`/`nextCursor` as JSON numbers**, while §8 item 16 says every list
  cursor is a string — and `lookup_symbol` accepts no `cursor` param, so the token can never be handed back.
  Folded into CR-14 rather than half-fixed.
