# F-TWO-SPELLINGS-OF-ONE-GIVEUP: the dispatched brief

> **Dispatch artifact, 2026-09-12 (overseer, session 7).** The row has no detail doc; its only record is the board
> title and the body of `8ae1dea` (2026-09-09), which booked it as a residual of the Screen-tab height-collapse fix.
> Premises measured at `41a2b10`: the Screen tab's give-up is `screen_room`
> (`crates/oracle-player/src/ui.rs:302`), returning `Result<egui::Vec2, &'static str>` with `Err(NO_ROOM_FOR_SCREEN)`,
> rendered through `no_picture(ui, why, readout)` (`:322`), and its doc names the defect it makes unrepresentable
> (`F-SCREEN-PICTURE-SILENT`: a zero from `screen_pick::fit` used to `return` and draw nothing). The plane viewer's
> picture is drawn by `Panels::plane_picture` through `plane_split` (`:2989`, called at `:1544` and `:7056`); per
> `8ae1dea` its give-up is an **inline check a future edit can drop**. Not located to the line before dispatch: finding
> it is step 1. `8ae1dea`'s other residual, the unbounded control strip, looks already answered by
> `SCREEN_STRIP_MAX_SHARE` (`:341`); report, do not change. A concurrent agent is on `parcel/cr-v-serve` in
> `crates/oracle-aether` (file-disjoint). Baseline `tools/land.sh --no-push` at `f7c0cd9` (everything since is docs):
> 89/89 legs, **release** 2858 passed / 0 failed / 3 ignored. The text below the rule is the agent prompt, passed with
> `isolation: worktree`.

---

You are an implementation agent in the **oracle** repo (a from-scratch Rust Mega Drive emulator; `crates/oracle-player` is its egui debug window). Parcel: **F-TWO-SPELLINGS-OF-ONE-GIVEUP**, size S. One branch.

**First, tell me where this brief is wrong.** Every mechanism below is a hypothesis; your command output outranks it.

## 0. Base check, branch, vendor, scratch

- `cd` to your worktree root (absolute path) before any git operation.
- Base check: `git merge-base --is-ancestor 41a2b10 HEAD && echo OK` must print OK and `docs/2026-09-12-one-giveup-spelling-brief.md` must exist; otherwise BLOCKED, stop.
- `git switch -c parcel/one-giveup-spelling`. Record your tip SHA as you go. **The controller (me) merges your branch into main and then deletes the branch and your worktree**; a missing branch afterwards is expected. Check `git merge-base --is-ancestor <tip> main` first, and confirm by content if that says no.
- `ln -s /home/volence/sonic_hacks/oracle/vendor vendor` (gitignored). `ls` is aliased to eza, so use `command ls`.
- Scratch: a run-unique dir you create, `/tmp/claude-1000/one-giveup-$(date +%s)/`.
- **If `origin/main` moves while you work**, `tools/land.sh` refuses at G3. Then `git fetch origin && git merge origin/main` into your branch (never rebase) and re-run.
- **Another agent is building in a sibling worktree at the same time** (`parcel/cr-v-serve`, in `crates/oracle-aether`). Do not touch that crate. The machine may be loaded; a test that fails only under load is a defect with a narrow window, not a flake: re-run it once to classify it, and report it by name either way.

## 1. The defect

Read `git show 8ae1dea` (its body is the whole booking). The Screen tab's "there is no room to draw the picture" give-up was made a `Result` (`screen_room`, `ui.rs` ~`:285-314`): the sentence travels with the failure, so a caller that handles it at all cannot handle it silently. The plane viewer (Planes tab, `Panels::plane_picture`, drawn through `plane_split`) has the same give-up spelled the older way, an inline check, so a future edit can delete the sentence and leave a blank pane that looks like a broken one. That is `F-SCREEN-PICTURE-SILENT`'s defect still reachable on the other tab.

**Step 1, establish it at HEAD before changing anything:** find every place in `crates/oracle-player` where a picture-drawing path gives up because there is no room (vary the search: `fit(`, `Vec2::ZERO`, `<= 0.0`, `available_size`, `available_width`, `return;` inside a picture function, the callers of `screen_pick::fit` and of `present::dest_rect`). For each one, say what the person sees today when it fires: a sentence, or nothing. If the plane viewer already says something, say exactly what and how it is reached. If you find more than two spellings, the parcel covers all of them; if the plane viewer turns out to have no give-up at all (it just draws zero pixels), that is the defect in its worst form, and the fix is the same.

## 2. The fix

**One owner for "the picture does not fit"**, used by both tabs (and any third site you find), shaped so the sentence cannot be separated from the failure: the `screen_room` `Result` shape is the precedent, and generalising it (or moving it somewhere both tabs can reach) is the likely answer, but it is your design; justify it. The plane viewer's sentence must tell a person what happened and what to do about it, in the same voice as `NO_ROOM_FOR_SCREEN`; if one sentence cannot serve both tabs truthfully, give the owner a parameter rather than writing a second copy. Keep `screen_pick::fit`'s arithmetic untouched: this parcel moves where the give-up is decided and said, not when a picture fits.

**Consuming surfaces:** doc comments that describe either give-up (`screen_room`'s, `plane_split`'s, `plane_picture`'s, `F-SCREEN-PICTURE-SILENT`'s mentions) must end true. List what you changed.

## 3. Tests

Headless only (`plane_split` was extracted so a headless test can drive it; there are existing egui tests in this crate: find and follow them). At minimum: a collapse on the Planes tab renders the sentence (assert the whole rendered sentence, not a fragment: it is text a person reads), and a collapse on the Screen tab still does. **Red-first:** a mutation that restores the old inline shape (drop the plane viewer's sentence and just return) must redden a named test; show the mutated line on disk before the red run.

## 4. Hard boundaries

- **Files in scope:** `crates/oracle-player/src/{ui.rs,screen_pick.rs,planes.rs}` and tests in `crates/oracle-player`. Anything else: say why in the report before touching it. Never `crates/oracle-aether`.
- Do not touch `docs/OVERSEER.md`, `docs/lane-status.json` or `docs/lens-findings.jsonl` (this is a board row, not a lens row).
- No change to when a picture fits, to any layout constant, or to the bus. No new look: the sentence renders the way `no_picture` already renders one.

## 5. Every check you add follows these five rules

(a) **red-first, SHOWING THE MUTATION APPLIED** (quote the mutated line from disk or `git diff --stat` before the red run); (b) restore from a **committed** baseline, never `git checkout --` over uncommitted work; (c) actually red: applied-and-still-green means the runner isn't executing your patch (cargo fingerprints by mtime; watch for "Compiling"); (d) wired into `cargo test --workspace`, expectations **derived** from the constants they test, never typed from the implementation's output, loud on unmeasurable; (e) if you tighten the method midway, re-establish earlier claims under it and say which. **For every assertion, ask: if this went green for a reason OTHER than the rule holding, what would that reason be?** Name it and say how you ruled it out (for example: the test pane was never small enough to collapse, so the sentence path never ran; assert the collapse happened as a precondition).

## 6. Running and verifying

- **One cargo invocation at a time in your worktree.** Targeted runs (`cargo test -p oracle-player`) while you work are fine.
- Final check: **`./tools/land.sh --no-push`** from your worktree root after your last commit, clean tree. **Detach it** per its header (run-unique wrapper in YOUR scratch dir writing `LAND-EXIT=$? AT $(date -Is)` to your log; `setsid nohup … &`), **then poll your own log in the FOREGROUND until LAND-EXIT appears**: `for i in $(seq 1 7); do grep -q '^LAND-EXIT=' "$LOG" && break; sleep 15; done`, repeated. **Do NOT end your turn while it runs, and never wait on a background-task notification.** Never foreground land.sh; never copy it over itself. Do not commit while it runs.
- Baseline, **release profile**: **89/89 legs, 2858 passed / 0 failed / 3 ignored**. **Predict your leg count and pass count before the run** and report prediction vs result.
- Report aggregate totals with the profile, never a tail; grep failures with `^test [^ ]+ \.\.\. FAILED`; capture exit codes outside pipes; match processes by exact name (`pgrep -x cargo`).
- Commit as each piece lands, **findings in the message**; messages from a file, read back with `git log -1 --format=%B`. Exact-path `git add` (never `-A`), `git show --stat` after each. No `Co-Authored-By`.

## 7. Standing rules

- **Never use any emulator or MCP tool** (`mcp__oracle__*`); **never launch a window on any display**, not even briefly. TAG anything that needs eyes on a real window.
- **BLOCKED is always available**: stop that item, record why, continue with the rest. Never degrade a design to reach green.
- Do not merge, push, or open a PR.

## 8. Report shape

1. Where this brief was wrong. 2. Branch, tip SHA (from git), commits. 3. Step 1's enumeration: every give-up site, what a person saw at each before. 4. The design, and why. 5. Tests by name, each mutation (line on disk → named failure → restore), the alternative green-path you ruled out. 6. `land.sh --no-push`: verdict token, LAND-EXIT, predicted vs observed legs, passed/failed/ignored, profile. 7. Open items (including the control-strip residual's status), TAGs, anything BLOCKED.
