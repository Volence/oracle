# LENS-WAVE3 (ONE-MASKED-FRAME, ONE-HEIGHT + BUTTON-TABLE): the dispatched brief

> **Dispatch artifact, 2026-09-12 (overseer, session 4).** Written from `docs/2026-09-11-lens-wave2-brief.md`'s shape.
> Premises re-measured at `4ff63fe` before dispatch: all three masked renderers present (`engine.rs:3649`
> `Engine::framebuffer(mask)`, `oracle-player/src/machine.rs:349` `render_masked`, `oracle-frontend/src/main.rs:729`
> `blit_masked`); the 224s present (`engine.rs:88`, frontend `main.rs:341`, player `machine.rs:19`, panels-spike
> `main.rs:59`, `system.rs:1378`/`:1388`, `testrom.rs:781`); `ui.rs:6649` `const ALL: [&str; 8]`; `preview.rs:336`
> `[false; 80]`. Baseline `tools/land.sh --no-push` at `ab1bb82` (4ff63fe is docs-only on top): 89/89 legs, release
> 2824 passed / 0 failed / 3 ignored. The text below the rule is the agent prompt, passed with `isolation: worktree`.

---

You are an implementation agent in the **oracle** repo (a from-scratch Rust Mega Drive emulator + its Aether debug bus). Parcel: **LENS-WAVE3**, the wave-3 parcels of the triage plan of record, `docs/2026-09-11-lens-triage.md` ("Wave 3" and the "Updated after …" addenda above it), plus one row half the earlier waves left in a file this parcel already owns. Size M. One branch, one commit set per row or tight group.

**First, tell me where this brief is wrong.** Every mechanism below is a hypothesis; your command output outranks it. The triage judged these rows from one-paragraph packet summaries (`docs/superpowers/notes/2026-09-06-oracle-lens-sweep.md`, entries M11, M31, M42, M53, M67, M69, M62), so **re-verify each row holds at HEAD before you fix it**. A row that no longer holds gets a ledger line (`state: "fixed"` with the commit that fixed it, or `"refuted"`) and no code. Earlier agents on this lane found the booked counts wrong in both directions; derive every population by varying the spelling AND the axis of your search (a literal, a derived expression, a comment that restates the fact, a test copy).

## 0. Base check, branch, vendor, scratch

- `cd` to your worktree root (absolute path) before any git operation.
- Base check: `git merge-base --is-ancestor 4ff63fe HEAD && echo OK` must print OK and `docs/2026-09-11-lens-triage.md` must exist; otherwise BLOCKED, stop.
- `git switch -c parcel/lens-wave3`. Record your tip SHA as you go. **The controller (me) merges your branch into main and then deletes the branch and your worktree**; a missing branch afterwards is expected — check `git merge-base --is-ancestor <tip> main` first, and confirm by content if that says no.
- `ln -s /home/volence/sonic_hacks/oracle/vendor vendor` (gitignored). `ls` is aliased to eza — use `command ls`.
- Scratch: a run-unique dir you create, `/tmp/claude-1000/lens-wave3-$(date +%s)/`.
- **If `origin/main` moves while you work**, `tools/land.sh` refuses at G3. Then `git fetch origin && git merge origin/main` into your branch (never rebase) and re-run.

## 1. Parcel A: ONE-MASKED-FRAME, ONE-HEIGHT (M11, M42, M31, M53/M67 consumer half)

**M11 (`F-THREE-MASKED-RENDERERS`).** `Engine::framebuffer(mask)` (oracle-aether `engine.rs`), `Machine::render_masked` (oracle-player `machine.rs`) and `blit_masked` (oracle-frontend `main.rs`) are three implementations of ONE masked picture across three crates, agreeing today with nothing asserting they must. The packet says the single owner of the frame geometry already exists and none of the three uses it (`Vdp::active_display()` in `oracle-core/src/render.rs`), and that `machine.rs` names this hazard in a comment and then walks into it a few lines later. Make the masked frame ONE implementation in `oracle-core` with the three sites as thin consumers (the frontend converts to its `u32` buffer at its own edge; that conversion is the frontend's, not the owner's). Same answer the tree already gave `sprite_tile_at` when it moved into `oracle-core`.

**The safety invariant you must keep, and why (read `docs/2026-08-26-layer-mask.md` for the record):**
- *Compiler-enforced:* no render that takes a `LayerMask` takes `&mut self`. Your new owner must be `&self` on `Vdp` (or a free fn over `&Vdp`), so it cannot compile a call to `Vdp::commit_scanline_sprites` (the `&mut self` write behind the sprite-overflow/collision latches). `LayerMask` stays a parameter, never a field.
- *Review-enforced only:* `render_scanline` (the stateful render) does NOT gain a mask parameter. Nothing in the language can carry this; the named test `masked_renders_leave_the_committed_sprite_latches_untouched` is the guard. Do not weaken it; extend it to cover your new owner if it does not already reach it.

**M42.** Each GUI path renders a full scanline just to learn a width that is `if h40 {320} else {256}`, every frame a mask is set. `Vdp::active_display()` is public and returns exactly that. Use it. The `if width == 0` guard beside it (commented as loud-on-unmeasurable's floor) is unreachable because width is always 256 or 320: remove it, or make it honest, and say which and why.

**M31.** With a mask set, every active line is composited TWICE per frame: once by the frame's own committed (unmasked) render and again for the masked display. The same loop is written in three places and none says the first render was paid for anyway. After M11 the loop exists once. **Establish from source whether the second composite is avoidable without a masked path committing state** (it must not: see the invariant). If it is not avoidable, the fix is the one loop plus a doc on the owner saying the cost is deliberate and why, and the row is closed on that. If you find it IS avoidable without breaking the invariant, BLOCKED-and-report with the evidence rather than building it — that is a design call for me.

**M53/M67 consumer half.** The owner is `oracle_core::vdp::ACTIVE_LINES: u16 = 224` (landed `d52a890`; the frame-end line and the active height are one fact). Make the consumers derive from it: `system.rs`'s `line < 224` / `line == 224` literals (≈`:1378`/`:1388`); the four production heights (`engine.rs` `ACTIVE_LINES`, frontend `main.rs` `HEIGHT`, player `machine.rs` `HEIGHT`, `oracle-panels-spike/src/main.rs` `HEIGHT` — the spike's deletion is an owner call, M8, so derive it, do not delete it); the test and example copies the M67 ledger row lists (read its latest line in `docs/lens-findings.jsonl`); and `testrom.rs`'s `PROF_VBLANK_LINE: u8 = 0xE0` — that one is a V-counter value in a generated test ROM, so tie it to `ACTIVE_LINES` by derivation or a compile-time assertion and say which. **NOT** `SCREEN_HEIGHT` in `oracle-aether/tests/object_mutation.rs` (~`:97`) or frontend `bus.rs` (~`:972`): that is aeon's player-bound inset, a different fact that happens to be 224. The wave-1C addendum also booked: **no oracle-core test catches a wrong `active_display()` height** — add one (mutate the height, show it red).

**Consuming surfaces, not only the code.** These rows repair CLAIMS as well as code. Comments that restate "three implementations", "the one masked picture", "224", "When V30 lands, it lands here", or the hazard note in `machine.rs` must end true. This lane's measured lesson: a claim repaired at its canonical site leaves its paraphrases standing, and the paraphrases are where the next parcel falsifies it. Enumerate them by varying spelling and axis, and list what you changed.

**The gate is picture parity, and it must be able to fail.** This parcel is picture-neutral by construction, so prove it. BEFORE you refactor, write a test that pins each of the three current implementations against an INDEPENDENT expectation, built per line from `oracle-core`'s `render_line_masked`, not from any of the three. Run it over H32 and H40 and over a varied set of masks, including all-off, each layer alone, and all-on. Then refactor, and keep the test as each consumer versus the owner. **Two warnings, both paid for on this lane.** (1) A sweep comparing two paths stays GREEN when both are wrong the same way. Assert a control BEFORE the sweep: a fixture whose masked and unmasked pictures differ, with the pixel where they differ derived rather than picked. Otherwise a mask that does nothing passes. (2) A coverage assertion that names an axis, like `saw_h32 && saw_h40`, is not coverage of the mechanism. Show one mutation per consumer reddening the test by name, for example the owner ignoring one layer, or a consumer using the wrong width.

**Currency is untouched:** zero diff under `crates/oracle-core/tests/` golden files, and `state_hash`/`export_state` outputs unchanged. `system.rs`'s edit is literal → constant only.

## 2. Parcel B: BUTTON-TABLE (M69 player half), and M62's player half

- **M69.** `oracle-player/src/ui.rs` (~`:6649`) carries `const ALL: [&str; 8] = ["up", …, "start"]`, a copy of `oracle_aether::engine::BUTTONS_3` (made `pub` at `6dce733`). Row title: *a duplicated button table degrades a negative control rather than reddening it*. Import the owner. Red-first: show that a mutation to `BUTTONS_3` (on disk, quoted) now reddens that control by name where before it would have degraded silently.
- **M62 (player half).** `oracle-player/src/preview.rs` (~`:336`) `let mut seen = [false; 80];` should size from `oracle_core::vdp::SAT_SLOTS`. The row closes when this lands (engine half `6dce733`, vdp half wave 1C). The test-module `walk(…, 80)` literals are H40 walk limits; convert them too if they mean the SAT slot count, and say which you converted.

## 3. Hard boundaries

- **Files in scope:** `crates/oracle-core/src/render.rs` and/or `src/vdp.rs` (the new `&self` owner and its tests only — no change to any `&mut self` render, to `commit_scanline_sprites`, or to emulation behaviour), `src/system.rs` (the two literals), `src/testrom.rs` (`PROF_VBLANK_LINE`); `crates/oracle-aether/src/engine.rs` (`framebuffer`, `ACTIVE_LINES`); `crates/oracle-player/src/{machine.rs,bus.rs,ui.rs,preview.rs}`; `crates/oracle-frontend/src/{main.rs,drain.rs}`; `crates/oracle-panels-spike/src/main.rs`; the test/example copies M67 names; new tests anywhere sensible. Anything else: say why in the report before touching it.
- Do not touch `docs/OVERSEER.md` or `docs/lane-status.json` (the overseer's files; the second is parsed by a console outside this repo). Do not touch the vendored contract/schema.
- **No wire change**: no new result key, no changed reply shape or text on the bus. If a row turns out to need one, BLOCKED-and-report that row.

## 4. Every check you add follows these five rules

(a) **red-first, SHOWING THE MUTATION APPLIED** (quote the mutated line from disk or `git diff --stat` before the red run); (b) restore from a **committed** baseline, never `git checkout --` over uncommitted work; (c) actually red — applied-and-still-green means the runner isn't executing your patch (cargo fingerprints by mtime; watch for "Compiling"); (d) wired into `cargo test --workspace`, expectations **derived** from source/constants, never from the implementation under test, loud on unmeasurable (never render "couldn't measure" as 0 or green); (e) if you tighten the method midway, re-establish earlier claims under it and say which were re-established and how.

## 5. Running and verifying

- **One cargo invocation at a time in this repo.** Targeted runs (`cargo test -p <crate> --test <file>` / `--lib`) while you work are fine. A test that fails only under machine load is a defect with a narrow window, not a flake: report it by name.
- Final check: **`./tools/land.sh --no-push`** from your worktree root after your last commit, clean tree. **Detach it** per its header (run-unique wrapper in YOUR scratch dir writing `LAND-EXIT=$? AT $(date -Is)` to your log; `setsid nohup … &`), **then poll your own log in the FOREGROUND until LAND-EXIT appears**: `for i in $(seq 1 7); do grep -q '^LAND-EXIT=' "$LOG" && break; sleep 15; done`, repeated. **Do NOT end your turn while it runs, and never wait on a background-task notification.** Never foreground land.sh; never copy it over itself.
- Baseline, **release profile**, from `tools/land.sh --no-push` at `ab1bb82`: **89/89 legs, 2824 passed / 0 failed / 3 ignored**. **Predict your leg count and pass count before the run** and report prediction vs result; a moved leg count (a new test target) is an explanation or a problem — say which.
- Report aggregate totals with the profile, never a tail; grep failures with `^test [^ ]+ \.\.\. FAILED`; capture exit codes outside pipes; match processes by exact name (`pgrep -x cargo`).
- Commit as each row lands, **findings in the message** (a death must cost the run, never the work); messages from a file, read back with `git log -1 --format=%B`. Exact-path `git add` (never `-A`), `git show --stat` after each. No `Co-Authored-By`.
- Ledger: for each lens row you fix, append ONE line to `docs/lens-findings.jsonl` copying that row's shape (`id, sweep, seat, severity, title` from its latest line; `state: "fixed"`, `fixedAt` from `git rev-parse`, `at` from `date -u +%Y-%m-%dT%H:%M:%SZ`, `detail`); a row left partly open gets `state: "open"` with the residue named. Append only; validate it parses. Fix commit first, ledger commit after.

## 6. Standing rules

- **Never use any emulator or MCP tool** (`mcp__oracle__*`); never launch a window on any display. TAG anything needing a live run.
- **BLOCKED is always available** per row: stop that one, record why, continue with the rest. Never degrade a design to reach green.
- Do not merge, push, or open a PR.

## 7. Report shape

1. Where this brief was wrong. 2. Branch, tip SHA (from git), commits. 3. Per row: verdict at HEAD, what changed, tests added by name, each mutation (line on disk → named failure → restore), consuming surfaces changed. 4. `land.sh --no-push`: verdict token, LAND-EXIT, predicted vs observed legs, passed/failed/ignored (predicted vs actual), profile. 5. Open items, residue, new findings, TAGs, anything BLOCKED.
