# STYLE-NUMBER-BAKEOFF: delete treatment B from the Pacing tab (the dispatched brief)

> **Dispatch artifact, 2026-09-13 (overseer, session 20).** The ruling: **d-39 closed as `a-shared-card`**, the
> hub's pick under the owner's 2026-09-13T21:53:38Z delegation (*"I don't care about a or b they both look good,
> you can make the decision."*), overturnable by one word from him. Read firsthand at empyrean
> `579d485:docs/OVERSEER.md` line 89 (`579d485` is an ancestor of their `origin/main`; a log tick commit, so a
> revision to read the words at): *"(a) d-39 = `a-shared-card` ... B is deleted in one move per oracle's card."*
> Closure card: this repo's `docs/decisions.jsonl`, `d-39-answered`. The go is the hub's, by message at ~22:51Z,
> citing the same line plus `579d485:docs/OVERSEER.md:173` (a rebooted lane takes its own `next` row).
> Premises measured at oracle `91c755b`. `git diff --stat e4b4bbf 91c755b` names only docs, so the last full
> baseline stands: **release profile, `tools/land.sh --no-push` on `e4b4bbf`'s tree: 90/90 legs, 2907 passed /
> 0 failed / 3 ignored.**
> The text below the rule is the agent prompt, passed with `isolation: worktree`.

---

You are an implementation agent in the **oracle** repo (a from-scratch Rust Mega Drive emulator, its Aether debug bus, and `oracle-player`, the egui debug window). One small UI parcel, one branch: the owner's side was asked to choose between two ways of drawing a headline number on the Pacing tab. **A won. Delete B**, together with all the scaffolding that existed only to ask the question.

**First, tell me where this brief is wrong.** Everything below was read from source by the controller and not measured; your command output outranks it. The last nine agents in this repo each corrected their brief on material points. Say so first in your report.

## What exists today (read at `91c755b`)

All in `crates/oracle-player/src/ui.rs`:

- **The temporary block and its own deletion recipe**, ~3272-3296. The recipe for "if **bare** wins" is the one that applies: A is `StatShape::Bare`, "bare numbers sharing one card". Its text: *delete `StatShape` and `headline_comparison`, delete `stat`'s `Tile` arm and the `TILE_MIN_W` const, drop the `shape` parameter from `stat`/`stat_row` and their two call sites, and restore `Panels::pacing`'s one line to `card(ui, |ui| stat_row(ui, &r.headline))`.* It was written before the tab reached its present shape. **Check it against the tree rather than executing it blind.**
- **The temporary items**: `enum StatShape` (~3300), `headline_comparison` (~3313), `treatment_caption` (~3343), `HEADLINE_CHOICE` (~3365), `TREATMENT_A`/`TREATMENT_B` (~3371-3372), `TILE_MIN_W` (~3003, with its doc from ~2998), `stat`'s `Tile` arm (~3390), and `stat_row`'s `shape`-dependent gutter (~3450-3454). Bare's gutter is `COL_GUTTER * 2.0`.
- **Two call sites**, not one:
  - `Panels::pacing` ~1696: `headline_comparison(ui, &r.headline)`, with a comment above it (~1689-1695) that describes the comparison.
  - The audio section ~1723: `stat_row(ui, &a.stats, StatShape::Bare)`, with a comment (~1720-1722) that explains why it is *not* doubled. That reason dies with the comparison.
- **Docs that describe two shapes**: `stat`'s doc (~3384-3386, "Two shapes are drawn side by side right now"), `stat_body`'s doc (~3410, "identical in both treatments"), and `stat_row`'s doc (the gutter paragraph).
- **The tests that guard the comparison**: `mod stat_shape_tests` (~6982 onward): `both_treatments_draw_the_same_live_numbers` and `the_comparison_blocks_own_strings_keep_the_panel_rules`, plus anything else in the module. Its own doc says *"Temporary, and it dies with the block it guards."* It carries two reusable helpers, `drawn` (every string that reached the screen) and `headline` (a live `pacing::Readout` headline built from a real `Presents` meter rather than a hand-typed struct).

## What to do

1. **Delete B and the scaffolding** so the Pacing tab draws its headline exactly as A did: bare numbers inside one shared `card`, no captions, no standing question line, no second row. The audio section's `stat_row` stays, in the same bare shape it has today. **Everything the reader sees in A must be byte-identical afterwards.** The only thing that changes on screen is that the question line, both captions and the B row are gone. If the recipe would change A (a different gutter, a card lost or doubled, a different spacing), stop and report it.
2. **Rewrite the comments that described the comparison** so they describe what is there now. Do not leave a eulogy: one plain sentence where a reason is still needed (for example, why the headline sits on a shared card), nothing where none is.
3. **The guard.** The comparison's test dies with it, but its property is worth keeping in its landed form: **the Pacing tab's headline draws each live headline number and label exactly once.** Replace the module with the smallest test that proves this through **the panel's own drawing path**, not a copy of the call. If the only reachable path is `stat_row` itself, or building `Panels::pacing` headlessly takes more state than a small parcel warrants, say which, use the nearest path that is still the panel's, and state in the test's doc what it does *not* cover. Reuse `drawn` and `headline`. Keep the positive control (non-empty draw). The test must also fail if the scaffolding comes back. Prefer an observable over a string constant that no longer exists: for example, a count of 1 goes to 2 if a second row is re-added. Follow the gate rules below.
4. **Enumerate the consuming surfaces of "there are two treatments", by varying the spelling, not by grepping the words the canonical block uses.** This repo's measured lesson: a claim repaired at its canonical site leaves its paraphrases standing, one fix found eleven sites of one spelling and the next parcel found seven more of another. Vary it: `treatment`, `tile`, `Tile`, `bordered`, `StatShape`, `headline_comparison`, `comparison`, `look call 4`, `bare or`, `drawn twice`, `Drawn TWICE`, `temporary`, `one of them is going away`, across `crates/**` (code, comments, tests, and any `include_str!`'d doc) and `docs/**`. Report phrasings × hits × verdict (repair / historical, leave / not about this).
   - Known doc sites: `docs/2026-09-05-debug-window-audit.md` §6 parked look call 4 (~466-469) and the Planes parcel's "still parked" line (~585). **That page is stamped to a revision, so do not edit its existing lines.** Append one dated resolution note at the end of the page, or at the end of §6 if the page has an established append convention (check first). It should say call 4 is settled as bare, by whom (the hub under the owner's delegation), when, and at which commit B was deleted (say "this parcel's merge" and let me fill the SHA in if you cannot know it).
   - `docs/2026-09-06-queue-detail.md` and dated handoff and log docs are historical records: leave them.
5. **Better-than-the-floor pass, report only:** after the deletion, is anything in the tab now wrong that the comparison was masking? For example, the headline section has lost a heading it used to borrow from the question line, or the spacing above `governor` was tuned for two rows. Report, do not redesign. **Other panels adopting A is the next row (`DATA-DISPLAY-AUDIT`) and is out of scope here.**

## 0. Base check, branch, vendor, scratch

- `cd` to your worktree root (absolute path) before any git operation.
- Base check: `git merge-base --is-ancestor 91c755b HEAD && echo OK` must print OK, and this brief (`docs/2026-09-13-delete-b-brief.md`) must exist. If it is missing, run `git fetch origin && git merge --ff-only origin/main` once and re-check. Still missing means BLOCKED: stop.
- `git switch -c parcel/delete-b`. Record your tip SHA as you go. **I (the controller) merge your branch into main, then delete the branch and your worktree**, so a missing branch afterwards is expected. Check `git merge-base --is-ancestor <tip> main` first, and confirm by content if that says no (a squash merge rewrites commits).
- `ln -s /home/volence/sonic_hacks/oracle/vendor vendor` (gitignored), then check that `vendor/` resolves (17 TestRoms entries). Without it, 8 `save_state` rows fail and the 68000 SST sweep silently skips. `ls` is aliased to eza, so use `command ls`.
- Scratch: a run-unique directory you create, `/tmp/claude-1000/delete-b-$(date +%s)/`. Nothing of yours goes in any shared scratch path.

## Hard boundaries

- **Files in scope:** `crates/oracle-player/src/ui.rs`; the one appended note in `docs/2026-09-05-debug-window-audit.md`; comment-only repairs your item-4 sweep marks "repair" elsewhere in `crates/oracle-player`. **Anything else** (another crate, a behaviour change outside the Pacing headline, another panel) means stop first and say why.
- **Never touch** `docs/lane-status.json`, `docs/lane-log.jsonl`, `docs/decisions.jsonl`, `docs/lens-findings.jsonl`, any `docs/OVERSEER*.md`, or the vendored contract or schema. Queue outcomes go in your report, and I transcribe them.
- **No emulator, ever.** Never call any `mcp__oracle__*` tool, and never launch a window on any display. Headless `egui::Context` rendering in a test is fine. TAG anything that needs eyes on the real window.
- **BLOCKED is always available**: stop that item, record why, finish the rest. Never degrade a design to reach green.
- Do not merge, push, or open a PR.

## Every gate you add follows these rules

(a) **Red-first, with the mutation shown applied** before the red run: quote the mutated line from disk, or `git diff --stat` naming the file. (b) Restore from a **committed** baseline, never `git checkout --` over uncommitted work. (c) **Applied and still green is a finding about the instrument first**: cargo fingerprints by mtime, so watch for "Compiling", and fix the runner before claiming the gate. (d) Wired into a runner that executes it (name it), and loud on unmeasurable, never a silent 0. (e) If you tighten a method midway, re-establish earlier claims under it and say which. **For every "it went red" or "it stayed green", ask what else would produce the same result**, and say how you ruled it out. **Vary the mutation parameter** at least twice: for example, re-add a second `stat_row` of the headline, and separately feed the row something other than the live projection, or drop one stat. Do not repeat one mutation.

## Running and verifying

- **One cargo invocation at a time in your worktree.** No other agent is running cargo in this repo right now. Every timing figure ships with `cat /proc/loadavg` and wall-clock uptime, because peers share this box.
- Final check, after your last commit, on a clean tree: **`./tools/land.sh --no-push`** from your worktree root. **Detach it** per its header: a run-unique wrapper in YOUR scratch dir writing `LAND-EXIT=$? AT $(date -Is)` to your log, launched with `setsid nohup … &`. **Then poll your own log in the FOREGROUND until LAND-EXIT appears**: `for i in $(seq 1 7); do grep -q '^LAND-EXIT=' "$LOG" && break; sleep 15; done`, repeated. **Do NOT end your turn while it runs, and never wait on a background-task notification.** Never run land.sh in the foreground, and do not commit while it runs. The script is `tools/land.sh`. Read the `LAND-EXIT` token in the log, never a notification's exit code.
- Baseline (release profile, `tools/land.sh --no-push`): **90/90 legs, 2907 passed / 0 failed / 3 ignored.** Before the run, predict your legs and your passed/ignored counts: tests deleted and tests added, **by name**. Report prediction vs result, and name the profile on every count.
- Report aggregate totals, never a tail. Grep failures with `^test [^ ]+ \.\.\. FAILED`, capture exit codes outside pipes, and match processes by exact name (`pgrep -x cargo`).
- Commit as each piece lands, **with the findings in the message** (a death must cost the run, never the work). Write messages to a file and commit with `-F <file>` (never `-F -`, never backquotes inside `$(…)`), then read back with `git log -1 --format=%B`. Use exact-path `git add` (never `-A`), and run `git show --stat` after each commit. No `Co-Authored-By`.

## Report shape

1. Where this brief was wrong.
2. Branch, tip SHA (from git), commits, and `git diff --stat 91c755b...<tip>`.
3. What was deleted, and the evidence that A is unchanged on screen (how you established it, and what else could have made it look unchanged).
4. The guard: which path it drives, what it does not cover, and the mutations (shown on disk, red output quoted).
5. The consuming-surface sweep (phrasings × hits × verdict), and the audit-page note as appended.
6. The better-than-the-floor findings (item 5), report only.
7. `land.sh --no-push`: verdict token, LAND-EXIT, predicted vs observed legs, passed/failed/ignored, profile.
8. Open items and TAGs (anything only the owner's eyes can settle).
