# DMA-SRC-ADVANCE (cause A3): the dispatched brief

> **Dispatch artifact, 2026-09-12 (overseer, session 10).** Second fix of the board row VDP-PORT-ACCESS-FIXES,
> cause A3 of `docs/2026-09-12-vdp-port-access-full-rom.md`. Taken under the owner's standing "If something stops
> we have it work on the next item" (empyrean `origin/main` `db47b94`, `docs/OVERSEER.md` l.35) and the hub's
> rebooted-lane line (same file, l.135). Dispatched in parallel with VSRAM-DECODE (cause A1,
> `docs/2026-09-12-vsram-decode-brief.md`). Both move bytes, so they **land one after the other**, never
> together. Premises: no file under `crates/*/src` changed between the write-up's tip `dd4defe` and `965ab1a`.
> Baseline, release profile, `tools/land.sh --no-push` at `0f8d62e` (only notes since): **89/89 legs, 2878 passed /
> 0 failed / 3 ignored**. The text below the rule is the agent prompt.

---

You are an implementation agent in the **oracle** repo (a from-scratch Rust Mega Drive emulator and its Aether debug bus). Parcel: **DMA-SRC-ADVANCE**, size S, one branch, **one byte-moving fix**.

**First, tell me where this brief is wrong.** Every mechanism, line number and count below is a hypothesis. Your command output outranks it.

## The fix

Read `docs/2026-09-12-vdp-port-access-full-rom.md` first: sections "A2", "A3", "Proposed queue rows" and "The pin". The rule (Nemesis, *VDP Internals*, quoted there): **every DMA operation, fill and copy included, advances the lower two DMA source registers (21 and 22) by one per step, and decrements the length counter.** A fill never reads its source, but it still advances it. Ours: `run_fill` and `run_copy` (`crates/oracle-core/src/vdp.rs`) zero the length registers 19/20 and never touch 21/22.

Derive the exact arithmetic from the ROM's tables and state it: how many steps a fill or copy of a given length takes (and so how far 21/22 advance), whether 21 carries into 22, that **register 23 never takes the carry** (A2's rule: the source wraps inside its 128 KB page, so do not let your change carry into 23), and what a length of 0 (65,536) does. Tests 28 and 29 (ROM `$AA16` and `$AEC0`) are the acceptance tables: a 4-byte fill with source `$00FA` must leave register 21 at `$FE`.

Enumerate every path that completes a fill or a copy, by what CALLS the completion, not only by the two functions named. Say for each whether it now advances the source.

## What must flip and what must not

Predicted from the write-up's scratch experiment (row EXP=8): the ROM goes from **76/46/122 to 78/44/122**, failing the 46 minus **28 and 29**. Tests 25 and 26 must not move. Predict before you run, and report the prediction beside the result.

- **Re-derive `PORT_ACCESS_FAILING` from the run**, never type it from this brief. The same goes for the scorecard row's `all 22 pages cumulative=` tally.
- Add a direct unit test that a fill and a copy each advance registers 21/22 by the derived amount without carrying into 23. It is red-first (rules below).
- **Frozen currency: predicted byte-identical.** Under all five causes' rules together, the scratch experiment moved no frozen golden (`determinism_gate`, `export_state_v1`, `golden_frames`, `scanline_goldens`), and every scorecard row except `vdp_port_access` stayed byte-identical. Register values are machine state, so a game that reads back registers 21/22 after a fill could move. Any movement is either explained or a defect, so say which.
- Docs: append a dated line to the `vdp_port_access` entry in `docs/2026-07-25-testrom-conformance.md`, and a dated "Fixed" note under "A3" in `docs/2026-09-12-vdp-port-access-full-rom.md`. Append only. Never rewrite history in a dated doc.

## A second agent is working beside you

VSRAM-DECODE (cause A1) is being fixed at the same time on another branch. It changes the VSRAM address decode in `vdp.rs` (`read_target`, `write_target`, possibly a new read latch), and it too updates `PORT_ACCESS_FAILING` and the scorecard tally. **Do not touch VSRAM decoding**, and keep your edits to `run_fill`/`run_copy` confined to source-register handling. The fixes land one at a time. Whichever lands second will be asked to merge `main` and re-derive its pin, so expect that message and do not pre-empt it.

## Clean-room: binding

`crates/oracle-core/src/vdp.rs`'s module doc says: "no emulator source informs this code (clean-room, audit policy 3)". **Do not open, fetch, search or quote any emulator's source in any form.** That covers BlastEm, Genesis Plus GX, Ares, jgenesis, Exodus, MAME and this workspace's own `oracle-old` C++ core, including GitHub code search and blame views. **Allowed evidence:** the test ROM (disassemble it from `vendor/`), hardware documentation, and forum prose by hardware testers. WebFetch and WebSearch are for those docs and threads, never for code.

## 0. Base check, branch, vendor, scratch

- `cd` to your worktree root (absolute path) before any git operation.
- Base check: `git merge-base --is-ancestor 965ab1a HEAD && echo OK` must print OK, and `docs/2026-09-12-dma-src-advance-brief.md` must exist. If the file is missing, run `git fetch origin && git merge --ff-only origin/main` once and check again. If it is still missing, you are BLOCKED: stop.
- `git switch -c parcel/dma-src-advance`. Record your tip SHA as you go. **The controller (me) merges your branch into main and then deletes the branch and your worktree**, so a missing branch afterwards is expected. Check with `git merge-base --is-ancestor <tip> main` first. If that says no, confirm by content, because a squash merge rewrites commits.
- `ln -s /home/volence/sonic_hacks/oracle/vendor vendor` (gitignored). Without it the ROM tests print SKIP and pass, which is a silent false green. **Confirm the full-ROM test actually ran by its output, not by `ok`.** `ls` is aliased to eza, so use `command ls`.
- Scratch: a run-unique dir you create, `/tmp/claude-1000/dma-src-advance-$(date +%s)/`.
- **If `origin/main` moves while you work**, `tools/land.sh` refuses at G3. Then run `git fetch origin && git merge origin/main` into your branch (never rebase) and re-run.

## Hard boundaries

- **Files in scope:** `crates/oracle-core/src/vdp.rs` (and `bus.rs` only if your enumeration shows a completion path there; say why); `crates/oracle-core/tests/conformance_roms.rs`; the two docs named above. Anything else: stop that item and report.
- **Never touch** `docs/lane-status.json`, `docs/lane-log.jsonl`, `docs/decisions.jsonl`, `docs/lens-findings.jsonl`, any `docs/OVERSEER*.md`, or the vendored contract or schema.
- **No emulator, ever.** Never call any `mcp__oracle__*` tool. Never launch a window or BlastEm on any display. Drive the ROM headless through `oracle_core::System` in tests, as the existing tests do. TAG anything that needs a live look.
- **BLOCKED is always available**: stop that item, record why, finish the rest. Never degrade the design to reach green.
- Do not merge, push, or open a PR.

## Every check you add follows these five rules

(a) **Red-first, SHOWING THE MUTATION APPLIED**: quote the mutated line from disk, or `git diff --stat`, before the red run. Natural mutations: drop the advance from the fill only, then from the copy only, and show which tests go red by name. Then let the advance carry into register 23 and show what catches it. (b) Restore from a **committed** baseline, never with `git checkout --` over uncommitted work. (c) Actually red: a mutation that is applied and still green means the runner is not executing your patch. Cargo fingerprints by mtime, so watch for "Compiling". (d) Wired into `cargo test --workspace`, with expectations **derived** from the ROM's tables and the rule, and loud when something cannot be measured. (e) If you tighten the method midway, re-establish the earlier claims under it and say which ones. **For every assertion, ask: if this went green for a reason OTHER than the claim holding, what would that reason be?** Name it in the report and say how you ruled it out.

## Running and verifying

- **One cargo invocation at a time in your worktree.** Use targeted runs while you work (`cargo test -p oracle-core --test conformance_roms vdp_port_access`, `cargo test -p oracle-core --lib vdp`). A test that fails only under machine load is a defect with a narrow window, not a flake. Report it by name.
- Final check: **`./tools/land.sh --no-push`** from your worktree root after your last commit, on a clean tree. **Detach it** per its header: a run-unique wrapper in YOUR scratch dir that writes `LAND-EXIT=$? AT $(date -Is)` to your log, started with `setsid nohup … &`. **Then poll your own log in the FOREGROUND until LAND-EXIT appears**: `for i in $(seq 1 7); do grep -q '^LAND-EXIT=' "$LOG" && break; sleep 15; done`, repeated. **Do NOT end your turn while it runs, and never wait on a background-task notification.** Never run land.sh in the foreground. Do not commit while it runs.
- Baseline, **release profile**, `tools/land.sh --no-push` at `0f8d62e`: **89/89 legs, 2878 passed / 0 failed / 3 ignored**. **Predict your leg and pass counts before the run**, and report the prediction beside the result. A moved leg count needs an explanation, or it is a problem, so say which.
- Report aggregate totals with the profile, never a tail. Grep failures with `^test [^ ]+ \.\.\. FAILED`. Capture exit codes outside pipes. Match processes by exact name (`pgrep -x cargo`).
- Commit as each piece lands, **with the findings in the message**, so a death costs the run and never the work. Write messages from a file and read them back with `git log -1 --format=%B`. Use exact-path `git add` (never `-A`) and run `git show --stat` after each commit. No `Co-Authored-By`.

## Report shape

1. Where this brief was wrong. 2. Branch, tip SHA (from git), commits. 3. The arithmetic derived from the ROM (steps, carry, register 23, length 0) and the completion-path enumeration. 4. The ROM before and after: tally, failing set, prediction against result. 5. Tests added, each with its mutation (line on disk, then the named failure, then the restore) and the alternative green path ruled out. 6. Frozen currency: every golden and currency constant, byte-identical or explained. 7. `land.sh --no-push`: verdict token, LAND-EXIT, predicted and observed legs, passed/failed/ignored, profile. 8. Open items, TAGs, anything BLOCKED.
