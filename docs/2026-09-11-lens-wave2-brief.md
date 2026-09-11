# LENS-WAVE2 (AETHER-HANDLERS): the ready-to-dispatch brief

> **Handoff artifact, 2026-09-11 (overseer).** Written at the end of a long session so the next one can dispatch
> wave 2 without re-deriving it. Before dispatching: (1) **re-measure every row's premise** — check whether the WORK
> landed, not whether the ROW is open (`docs/2026-09-11-lens-triage.md` is the plan of record, with its two
> addenda); (2) fill `__BASE__` with `origin/main` and `__BASELINE__` with the latest `tools/land.sh` release counts
> (legs / passed / failed / ignored, the profile named); (3) wave 1C (`parcel/lens-wave1-c`) must have LANDED first,
> since this brief assumes its `SAT_SLOTS` owner. Pass the text below as the agent prompt, with `isolation: worktree`.

---

You are an implementation agent in the **oracle** repo (a from-scratch Rust Mega Drive emulator + its Aether debug bus). Parcel: **LENS-WAVE2 — AETHER-HANDLERS**, the wave-2 parcel of the triage plan of record, `docs/2026-09-11-lens-triage.md`, plus two items later waves booked onto it. Every row lives in `crates/oracle-aether/src/engine.rs` (with tests beside it). Done in one branch, one commit set per row or tight group. Size M.

**First, tell me where this brief is wrong.** Every mechanism below is a hypothesis; your command output outranks it. The triage was READ-ONLY and judged these rows from one-paragraph packet summaries, so **re-verify each row holds at HEAD before you fix it**. A row that no longer holds gets a ledger line (`state: "fixed"` with the commit that fixed it, or `"refuted"`) and no code. Earlier agents on this lane found counts wrong in their rows; derive every population by varying the spelling and the axis of your search.

## 0. Base check, branch, vendor, scratch

- `cd` to your worktree root (absolute path) before any git operation.
- Base check: `git merge-base --is-ancestor __BASE__ HEAD && echo OK` must print OK and `docs/2026-09-11-lens-triage.md` must exist; otherwise BLOCKED, stop.
- `git switch -c parcel/lens-wave2`. Record your tip SHA. **The controller (me) merges your branch into main and then deletes the branch and your worktree**; a missing branch afterwards is expected — check `git merge-base --is-ancestor <tip> main` first.
- `ln -s /home/volence/sonic_hacks/oracle/vendor vendor` (gitignored). `ls` is aliased to eza — use `command ls`.
- Scratch: a run-unique dir you create, `/tmp/claude-1000/lens-wave2-$(date +%s)/`.
- **If `origin/main` moves while you work**, `tools/land.sh` refuses at G3. Then `git fetch origin && git merge origin/main` into your branch (never rebase) and re-run.

## 1. The rows (quoted from the plan and its two addenda; re-derive, don't trust)

**Refusals the contract asks for and the handler does not give:**
- **M25 (handler half)**: `emulator/z80_write` accepts `bytes: "0x"` (an empty payload) as an empty Ok — `hex::parse_bytes` returns empty, `z80_window(addr, 0)` passes, reply `{"len":0}` — while `write_memory` and `write_vram` refuse the same payload by name. Make it refuse the same way (`-32602` naming `bytes`). **In the SAME commit, delete the `emulator/z80_write` row from `KNOWN_ANSWERED` in `crates/oracle-aether/tests/request_shapes.rs`** — its anti-rot test (`the_registered_answered_sites_are_still_answered`) goes red the moment the handler refuses, by design, and only deleting the row turns it green. Show that red, then the green.
- **M17**: `object_list` / `object_slot` never insert the `caveat` their fragment declares for an indeterminate-but-intact symbol binding, though `load_symbols` accepts such tables. Emit it, conditionally, exactly when the fragment's description says; derive the condition from the fragment text (quote it), not from the handler.
- **M29**: `checkpoint_list.limit` is parsed with `cap = max_checkpoints` (a capacity) while the schema declares `minimum: 1` and NO maximum, so `limit: 100` is refused and `request_bounds` cannot see it. The recommended fix is code-only: treat `limit` as a page size (clamp to what exists; never refuse a number the schema allows). Declaring a maximum instead would be a contract change — do NOT do that.

**Names, summaries and helpers:**
- **M27**: `emulator/read`'s advertised `summary` says "one byte read across the bus/vram/cram/vsram spaces" while `len` takes up to 4096. It is visible text (the palette and `methodSummaries`) — make it true and short.
- **M73**: two `RpcError::new(code::INVALID_STATE, …)` bypasses (`perFrameNotArmed`, `callersNotArmed`) of the `RpcError::invalid_state` helper whose doc says it "can never be forgotten" (the triage measured 2 of 16 production sites; re-count). Route both through the helper, and make the bypass hard to reintroduce if there is a cheap structural way (say what you chose).
- **M41**: `all.sort_by_key(|s| (s.addr, s.name.clone()))` allocates a `String` per comparison; the only allocating key among the `sort_by_key` calls. Use a non-allocating form with identical ordering (prove the ordering is unchanged with a test that would catch a tie-break change).
- **L7 (engine half)**: `z80_read` bounds `len` with a literal `0x2000` while `Z80_RAM_SIZE` is imported and used elsewhere in the file. Use the constant. (The frontend's `MAX_SYMBOL_DISPLACEMENT` copies are NOT in scope — they retire with the frontend.)
- **M69 (engine half)**: make `BUTTONS_3` public so `oracle-player/src/ui.rs` can import it later (the `ui.rs` half is a later parcel; do not touch `ui.rs`).
- **M62 (engine half)**: `engine.rs`'s `SAT_SLOTS` should import `oracle-core`'s owner (wave 1B put `pub const SAT_SLOTS` in `render.rs`; wave 1C may have moved it to `vdp.rs` with a re-export — use whatever `main` has). The row closes only if `oracle-player/src/preview.rs`'s `[false; 80]` is also done; leave that file alone and say what remains.
- **M23 (row-index half)**: `parse_input_rows` drops the row index when `parse_port(row)?` / `parse_buttons(row)?` fail, so an error on row 7 of 40 does not say which row. Add the index to the error. (Closing `play_input.rows.items`' key set is a contract change — NOT in scope.) Also note: `parse_port` returns `Ok(0)` for an absent `port`; report whether that default is what the fragment says, and do not change it unless the fragment says otherwise.

**The new finding booked onto this wave (not from the sweep, so no ledger row exists):**
- `Engine::lookup_symbol`'s demangled branch reports `ambiguous: true` and the caveat *"N different addresses answer…"* for a **same-address alias group**, which is false by `crates/oracle-core/src/symbols.rs`'s own rule (same-address aliases are deliberately not ambiguous; see `Resolution::name()`'s doc, which wave 1A corrected). Latent: no frozen listing has such a group, so build the case in a test. Fix it so an alias group at one address answers unambiguously; this changes a reply only for an input no real listing has produced — say so. No ledger line (it has no id); report it.

## 2. Hard boundaries

- **Files: `crates/oracle-aether/src/engine.rs` and aether tests.** Do not touch `crates/oracle-player/` (any file), `crates/oracle-core/src/system.rs`, `crates/oracle-core/src/vdp.rs`, `docs/OVERSEER.md`, or `docs/lane-status.json` (the overseer's file, parsed by a console outside this repo). The ONE edit outside `engine.rs` besides new tests is deleting the `KNOWN_ANSWERED` row (M25).
- **Currency is untouched**: zero diff under `crates/oracle-core/tests/` goldens.
- **No new result key and no changed reply shape.** M17 emits a caveat its fragment already declares; M25/M23 change refusal texts and codes toward what the contract already says; M27 changes a summary string. Name each of those in your report. Anything that needs a contract change is BLOCKED-and-report.

## 3. Every check you add follows these five rules

(a) **red-first, SHOWING THE MUTATION APPLIED** (quote the mutated line from disk or `git diff --stat` before the red run); (b) restore from a **committed** baseline, never `git checkout --` over uncommitted work; (c) actually red — applied-and-still-green means the runner isn't executing your patch (cargo fingerprints by mtime; watch for "Compiling"); (d) wired into `cargo test --workspace`, expectations **derived** from the vendored schema `crates/oracle-aether/tests/contract/bus-protocol.schema.json` or from source, never from the handler under test, loud on unmeasurable; (e) if you tighten the method midway, re-establish earlier claims under it and say so. A refusal test must assert the refusal names the field it is about (a matcher that two different refusals satisfy proves nothing).

## 4. Running and verifying

- **One cargo invocation at a time in this repo.** Targeted runs (`cargo test -p oracle-aether --test <file>`) while you work are fine. A test that fails only under machine load is a defect with a narrow window, not a flake: report it by name.
- Final check: **`./tools/land.sh --no-push`** from your worktree root after your last commit, clean tree. **Detach it** per its header (run-unique wrapper in YOUR scratch dir writing `LAND-EXIT=$? AT $(date -Is)` to your log; `setsid nohup … &`), **then poll your own log in the FOREGROUND until LAND-EXIT appears**: `for i in $(seq 1 7); do grep -q '^LAND-EXIT=' "$LOG" && break; sleep 15; done`, repeated. **Do NOT end your turn while it runs.** Never foreground land.sh; never copy it.
- Baseline, release profile, from `tools/land.sh` on main at __BASE__: __BASELINE__. **Predict your pass count before the run** and report prediction vs result.
- Report aggregate totals with the profile, never a tail; grep failures with `^test [^ ]+ \.\.\. FAILED`; capture exit codes outside pipes; match processes by exact name (`pgrep -x cargo`).
- Commit as each row lands, **findings in the message**; messages from a file, read back with `git log -1 --format=%B`. Exact-path `git add`, `git show --stat` after each. No `Co-Authored-By`.
- Ledger: for each lens row you fix, append ONE line to `docs/lens-findings.jsonl` copying that row's shape (`id, sweep, seat, severity, title` from its latest line; `state: "fixed"`, `fixedAt` from `git rev-parse`, `at` from `date -u +%Y-%m-%dT%H:%M:%SZ`, `detail`); a row left partly open gets `state: "open"` with the residue named. Append only; validate it parses. Fix commit first, ledger commit after.

## 5. Standing rules

- **Never use any emulator or MCP tool** (`mcp__oracle__*`). TAG anything needing a live run.
- **BLOCKED is always available** per row: stop that one, record why, continue. Never degrade a design to reach green.
- Do not merge, push, or open a PR.

## 6. Report shape

1. Where this brief was wrong. 2. Branch, tip SHA (from git), commits. 3. Per row: verdict at HEAD, what changed, tests added by name, each mutation (line on disk → named failure → restore), and any wire-visible text change. 4. `land.sh --no-push`: verdict token, LAND-EXIT, derived vs observed legs, passed/failed/ignored (predicted vs actual), profile. 5. Open items, residue, new findings, TAGs, anything BLOCKED.
