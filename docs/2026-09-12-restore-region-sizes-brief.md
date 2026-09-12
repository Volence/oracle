# RESTORE-REGION-SIZES: the dispatched brief

> **Dispatch artifact, 2026-09-12 (overseer, session 6).** Residue (1) of the LENS-WAVE4 addendum in
> `docs/2026-09-11-lens-triage.md` ("Updated after wave 4"). Code-only. Runs beside the STATE-HASH-TYPED-MASK CR
> draft, which is docs-only and runs no cargo, so the two cannot collide.
> Premises measured at oracle `6b5b5c1`: `System::restore` (`crates/oracle-core/src/system.rs:843`) is
> `bincode::decode_from_slice(...)?` and nothing else. `Vdp` keeps `vram`/`cram`/`vsram` as `Vec<u8>` (allocated at
> `vdp.rs:406-411`) and since wave 4 hands them out as fixed arrays through `vdp.rs:453-469`, each an `.expect(...)`
> on the length. So a snapshot whose decoded region has the wrong length restores "successfully" and panics at the
> first accessor. Callers of `System::restore`: `crates/oracle-aether/src/engine.rs:8158` (checkpoint restore,
> in-process bytes), `crates/oracle-frontend/src/save_state.rs:243` (a save-state file off disk: the untrusted
> door), `crates/oracle-core/examples/sh_probe.rs:28`, `crates/oracle-core/tests/proptests.rs:45`. Baseline, release
> profile, `tools/land.sh --no-push` at `3b825f5` (everything after it is docs-only): 89/89 legs, 2841 passed /
> 0 failed / 3 ignored. The text below the rule is the agent prompt, passed with `isolation: worktree`.

---

You are an implementation agent in the **oracle** repo (a from-scratch Rust Mega Drive emulator and its Aether debug bus). Parcel: **RESTORE-REGION-SIZES**, size S, one branch.

**First, tell me where this brief is wrong.** Every mechanism below is a hypothesis; your command output outranks it. Earlier agents on this lane found booked counts wrong in both directions.

## The defect

`System::restore(bytes)` decodes a bincode snapshot and returns the machine without checking that any fixed-size region decoded at its fixed size. The VDP's three memories are the booked case: they are `Vec<u8>` fields, and their accessors `expect` the right length, so a malformed snapshot panics at the first read instead of being refused at the door. The one caller that reads bytes nobody in this process produced is the frontend's save-state load from a file on disk (`oracle-frontend/src/save_state.rs`); a truncated or hand-edited slot file should come back as an error the window can show, not a crash.

## What to do

1. **Enumerate every region the invariant covers, by what TOUCHES the data, not by the three names above.** Every `Vec` (or other length-carrying) field reachable from `System` whose length the code elsewhere assumes: an `expect`, an index, a `debug_assert`, a `try_into`, a constant it is allocated at. Candidates to check, not a list to transcribe: VDP VRAM/CRAM/VSRAM and the register file, work RAM, Z80 RAM, SRAM and its map, the cartridge ROM image and bank table, any FIFO or ring. Say for each whether a wrong length can decode, and what happens next.
2. **Refuse at the door.** `restore` returns `Err` naming the region and both lengths (expected vs found) for every region whose wrong length would break a later assumption. Design the error type (bincode's own error, a new `RestoreError`, or whatever fits the workspace), and follow each caller's mapping so the message a person reads still says what went wrong. Keep `snapshot`'s bytes unchanged: **no snapshot format change, no `export_state` change, no `state_hash` change**, and every golden file byte-identical.
3. **Consumers.** Update every caller the signature change forces, and check the player window's own save-state path too (find how `crates/oracle-player` loads a state; it may reach `restore` through another crate). Doc comments that say what `restore` guarantees must end true, including paraphrases elsewhere (the frontend's `save_state.rs` module doc discusses `System::restore` at length).
4. **Tests.** For each refused region, a test that builds a real snapshot, corrupts that region's length in a way that still decodes (derive how from the encoding, e.g. decode, shrink the `Vec`, re-encode, or edit the length prefix, and say which), and asserts the refusal names that region. Plus the control: an unmodified snapshot still restores and round-trips. Plus one through the untrusted door: a malformed slot file handed to the frontend's load path comes back as the error, not a panic.

## 0. Base check, branch, vendor, scratch

- `cd` to your worktree root (absolute path) before any git operation.
- Base check: `git merge-base --is-ancestor 6b5b5c1 HEAD && echo OK` must print OK and `docs/2026-09-12-restore-region-sizes-brief.md` must exist; otherwise BLOCKED, stop.
- `git switch -c parcel/restore-region-sizes`. Record your tip SHA as you go. **The controller (me) merges your branch into main and then deletes the branch and your worktree**; a missing branch afterwards is expected. Check `git merge-base --is-ancestor <tip> main` first, and confirm by content if that says no.
- `ln -s /home/volence/sonic_hacks/oracle/vendor vendor` (gitignored). `ls` is aliased to eza, so use `command ls`.
- Scratch: a run-unique dir you create, `/tmp/claude-1000/restore-sizes-$(date +%s)/`.
- **If `origin/main` moves while you work**, `tools/land.sh` refuses at G3. Then `git fetch origin && git merge origin/main` into your branch (never rebase) and re-run.

## Hard boundaries

- **Files in scope:** `crates/oracle-core/src/` (system.rs and whatever region owner needs a check), every caller the signature forces (`crates/oracle-aether/src/engine.rs`, `crates/oracle-frontend/src/save_state.rs`, `crates/oracle-core/examples/sh_probe.rs`, `crates/oracle-core/tests/proptests.rs`, and any the player needs), and new tests anywhere sensible. Anything else: say why in the report before touching it.
- **Never touch** `docs/lane-status.json`, `docs/lane-log.jsonl`, `docs/decisions.jsonl` or any `docs/OVERSEER*.md` (the controller's files; the first is parsed by a console you cannot see), nor the vendored contract/schema.
- **No wire change.** `emulator/restore`'s replies and refusals stay byte-identical for every snapshot this process can produce (a checkpoint is always well-formed). If the error text of an existing wire refusal would change, show that it is unreachable from the wire or BLOCKED-and-report.
- **No emulation behaviour change.** Only the refusal of malformed input is new.
- **No emulator, ever.** Never call any `mcp__oracle__*` tool; never launch a window on any display. TAG anything needing a live run.
- **BLOCKED is always available**: stop that item, record why, finish the rest. Never degrade a design to reach green.
- Do not merge, push, or open a PR.

## Every check you add follows these five rules

(a) **red-first, SHOWING THE MUTATION APPLIED** (quote the mutated line from disk or `git diff --stat` before the red run). The natural mutation is deleting or weakening the refusal for one region; show the corrupted-snapshot test then panics or passes wrongly, by name. (b) Restore from a **committed** baseline, never `git checkout --` over uncommitted work. (c) Actually red: applied-and-still-green means the runner isn't executing your patch (cargo fingerprints by mtime; watch for "Compiling"). (d) Wired into `cargo test --workspace`, expectations **derived** from the region constants, never from the implementation under test; loud on unmeasurable. (e) If you tighten the method midway, re-establish earlier claims under it and say which. **For every assertion, ask: if this went green for a reason OTHER than the rule holding, what would that reason be?** (For example: the corrupted snapshot failed to decode at all, so bincode refused it and your check never ran. Assert WHICH refusal fired.) Name it in the report and say how you ruled it out.

## Running and verifying

- **One cargo invocation at a time in this repo.** Targeted runs (`cargo test -p <crate> --test <file>` / `--lib`) while you work are fine. A test that fails only under machine load is a defect with a narrow window, not a flake: report it by name.
- Final check: **`./tools/land.sh --no-push`** from your worktree root after your last commit, clean tree. **Detach it** per its header (run-unique wrapper in YOUR scratch dir writing `LAND-EXIT=$? AT $(date -Is)` to your log; `setsid nohup … &`), **then poll your own log in the FOREGROUND until LAND-EXIT appears**: `for i in $(seq 1 7); do grep -q '^LAND-EXIT=' "$LOG" && break; sleep 15; done`, repeated. **Do NOT end your turn while it runs, and never wait on a background-task notification.** Never foreground land.sh; never copy it over itself. Do not commit while it runs.
- Baseline, **release profile**, `tools/land.sh --no-push` at `3b825f5`: **89/89 legs, 2841 passed / 0 failed / 3 ignored**. **Predict your leg count and pass count before the run** and report prediction vs result; a moved leg count is an explanation or a problem, so say which.
- Report aggregate totals with the profile, never a tail; grep failures with `^test [^ ]+ \.\.\. FAILED`; capture exit codes outside pipes; match processes by exact name (`pgrep -x cargo`).
- Commit as each piece lands, **findings in the message** (a death must cost the run, never the work); messages from a file, read back with `git log -1 --format=%B`. Exact-path `git add` (never `-A`), `git show --stat` after each. No `Co-Authored-By`.

## Report shape

1. Where this brief was wrong. 2. Branch, tip SHA (from git), commits. 3. The region enumeration: each region, whether a wrong length could decode, what happened next before, what happens now. 4. Tests added by name, each mutation (line on disk → named failure → restore), the alternative green path you ruled out. 5. Consuming surfaces changed (callers, docs, paraphrases). 6. `land.sh --no-push`: verdict token, LAND-EXIT, predicted vs observed legs, passed/failed/ignored, profile. 7. Open items, new findings, TAGs, anything BLOCKED.
