# DMA-SRC-128K (cause A2): the dispatched brief

> **Dispatch artifact, 2026-09-12 (overseer, session 11).** Third fix of the board row
> VDP-PORT-ACCESS-FIXES, cause A2 of `docs/2026-09-12-vdp-port-access-full-rom.md`. Taken under the
> owner's standing 2026-09-11T18:23:49Z instruction and the hub's rebooted-lane line, both verified
> firsthand at empyrean `origin/main` `8fe1122`, `docs/OVERSEER.md` l.73-80 and l.128-135. Dispatched in
> parallel with FILL-BUSY-ARM + FILL-TGT (causes A4 and A5,
> `docs/2026-09-12-fill-busy-and-target-brief.md`). Both move bytes, so they **land one after the other**,
> never together. Premises: A1 (VSRAM-DECODE) and A3 (DMA-SRC-ADVANCE) are already on `main` at `d34f37c`;
> the ROM stands at **114/8/122**, failing **20 27 31 32 33 34 36 38** (read from `PORT_ACCESS_FAILING` at
> `d34f37c`, not from the write-up). Baseline, release profile, `tools/land.sh --no-push` on the A1 merge:
> **89/89 legs, 2884 passed / 0 failed / 3 ignored**. Both CI runs for `e3a4456` and `d34f37c` are SUCCESS
> on all three jobs. The text below the rule is the agent prompt.

---

You are an implementation agent in the **oracle** repo (a from-scratch Rust Mega Drive emulator and its Aether debug bus). Parcel: **DMA-SRC-128K**, size S, one branch, **one byte-moving fix**.

**First, tell me where this brief is wrong.** Every mechanism, line number and count below is a hypothesis. Your command output outranks it. The last three agents on this board row each corrected their brief on at least one point; that is the expected outcome, not a failure.

## The fix

Read `docs/2026-09-12-vdp-port-access-full-rom.md` first: sections "A2", "A3", "The pin" and the site table under "Proposed queue rows". The rule, from MegaDrive Wiki *VDP / DMA Limitations* ("Every DMA cycle, only the low and middle bytes of the DMA source registers are incremented") and Plutiedev *DMA transfer* ("the source address can't cross a 128KB boundary"): **only source registers 21 and 22 advance. Register 23 never takes a carry, so a 68k-to-VDP DMA wraps inside its 128 KB page.**

Ours: `run_mem_dma` steps the source with `src = src.wrapping_add(2)` (`crates/oracle-core/src/bus.rs:1502`) and hands `src >> 1` to `Vdp::dma_complete` (`bus.rs:1513`), which writes the carry into register 23 (`vdp.rs:1298`). Both halves are hypotheses — verify the line numbers and the data flow yourself.

**A3 landed the same rule for fills and copies last session**, as `Vdp::advance_dma_source_low16` (registers 22:21 at `(start + steps) & $FFFF`, register 23 untouched). Read it before you write anything. Two outcomes are both acceptable and you choose with reasons stated: share that helper, or keep them separate because the 68k path differs. What is **not** acceptable is two functions in this file that implement the same hardware rule with different arithmetic.

## What must flip and what must not

Predicted, derived from the write-up's scratch matrix (EXP=7 against the current EXP=3+8 state): the ROM goes from **114/8/122 to 116/6/122**, failing **31 32 33 34 36 38**. Test 27 flips on A2 alone; **test 20 flips only because A1 is already in**, so it is a joint outcome, not evidence about your change alone — say so when you report it. Tests **24, 25, 26 and 42-71 must not move**. Predict before you run and report the prediction beside the result.

- **Re-derive `PORT_ACCESS_FAILING` from the run**, never type it from this brief. Same for the scorecard row's `all 22 pages cumulative=` tally.
- Add a direct unit test for the wrap rule: a 68k DMA whose source crosses a 128 KB boundary reads from the bottom of its own page, and register 23 is unchanged afterwards. Red-first, rules below.
- **Frozen currency: predicted byte-identical.** Under all five causes together the scratch experiment moved no frozen golden (`determinism_gate`, `export_state_v1`, `golden_frames`, `scanline_goldens`) and every scorecard row except `vdp_port_access` stayed byte-identical. Register 23 is machine state a game can read back, so movement is possible; any movement is either explained or a defect, and you say which.
- Docs: append a dated line to the `vdp_port_access` entry in `docs/2026-07-25-testrom-conformance.md`, and a dated "Fixed" note under "A2" in `docs/2026-09-12-vdp-port-access-full-rom.md`. **Append only. Never rewrite history in a dated doc.**

## The evidence to work from

**Test 20** (ROM `$60EA`): the VRAM and CRAM halves DMA 4 words from `$5FFFC`. The ROM holds `89ab cdef` at `$5FFFC`, `dead c0de` at `$60000`, `ffff eeee` at `$40000`. Hardware reads `89ab cdef ffff eeee`; ours reads `89ab cdef dead c0de`.

**Test 27** (ROM `$A5D0`): group 4 DMAs from `$5FFFC` across the boundary. Group 5 then rewrites registers 21 and 22 but **not** 23 (`$A96A..$A96C`). On hardware register 23 is still `$02`, so the DMA reads `$401FC` and gets `1111 ffff eeee dddd`. Ours took the carry to `$03`, reads `$601FC`, and gets `dead c0de dead c0de`.

Derive the wrap arithmetic from those tables and state it: where the wrap point sits in terms of registers 21/22/23, what a DMA that starts exactly on a boundary does, and what a length that would cross the boundary more than once does (the ROM may not cover that; if it does not, say so rather than inventing a table).

## A second agent is working beside you

FILL-BUSY-ARM + FILL-TGT (causes A4 and A5) are being fixed at the same time on another branch. That branch changes `control_write`'s arm path, the busy window behind the status bit, `run_fill`'s write-target decode and `target_of` — **all in `vdp.rs`**. You share the file. Keep your edits to the 68k DMA source path (`run_mem_dma`, `dma_complete`) and **do not touch the fill or copy paths, the busy window, or `target_of`**. You both re-pin `PORT_ACCESS_FAILING` and both append to the same two docs, so a conflict there is expected and is mine to resolve. Whichever lands second will be asked to merge `main` and re-derive its pin and tally; expect that message and do not pre-empt it.

## Clean-room: binding

`crates/oracle-core/src/vdp.rs`'s module doc says: "no emulator source informs this code (clean-room, audit policy 3)". **Do not open, fetch, search or quote any emulator's source in any form.** That covers BlastEm, Genesis Plus GX, Ares, jgenesis, Exodus, MAME and this workspace's own `oracle-old` C++ core, including GitHub code search and blame views. **Allowed evidence:** the test ROM (disassemble it from `vendor/`), hardware documentation, and forum prose by hardware testers. WebFetch and WebSearch are for those docs and threads, never for code.

## 0. Base check, branch, vendor, scratch

- `cd` to your worktree root (absolute path) before any git operation.
- Base check: `git merge-base --is-ancestor d34f37c HEAD && echo OK` must print OK, and `docs/2026-09-12-dma-src-128k-brief.md` must exist. If the file is missing, run `git fetch origin && git merge --ff-only origin/main` once and check again. If it is still missing, you are BLOCKED: stop.
- `git switch -c parcel/dma-src-128k`. Record your tip SHA as you go. **The controller (me) merges your branch into main and then deletes the branch and your worktree**, so a missing branch afterwards is the expected end state. Check with `git merge-base --is-ancestor <tip> main` first — and note that a non-ancestor result does **not** prove you were reaped, because a squash or rebase rewrites commits. Confirm by content.
- `ln -s /home/volence/sonic_hacks/oracle/vendor vendor` (gitignored). Without it the ROM tests print SKIP and pass, which is a silent false green. **Confirm the full-ROM test actually ran by its output, not by `ok`.** `ls` is aliased to eza, so use `command ls`.
- Scratch: a run-unique dir you create, `/tmp/claude-1000/dma-src-128k-$(date +%s)/`.
- **If `origin/main` moves while you work**, `tools/land.sh` refuses at G3. Then `git fetch origin && git merge origin/main` into your branch (never rebase) and re-run.

## Hard boundaries

- **Files in scope:** `crates/oracle-core/src/bus.rs`, `crates/oracle-core/src/vdp.rs` (the 68k DMA source path only), `crates/oracle-core/tests/conformance_roms.rs`, and the two docs named above. Anything else: stop that item and report.
- **Never touch** `docs/lane-status.json`, `docs/lane-log.jsonl`, `docs/decisions.jsonl`, `docs/lens-findings.jsonl`, any `docs/OVERSEER*.md`, or the vendored contract or schema.
- **No emulator, ever.** Never call any `mcp__oracle__*` tool. Never launch a window or BlastEm on any display. Drive the ROM headless through `oracle_core::System` in tests, as the existing tests do. TAG anything that needs a live look for my foreground follow-up.
- **BLOCKED is always available**: stop that item, record why, finish the rest. Never degrade the design to reach green.
- Do not merge, push, or open a PR.

## Every check you add follows these five rules

(a) **Red-first, SHOWING THE MUTATION APPLIED**: quote the mutated line from disk, or `git diff --stat`, before the red run. Natural mutations: restore the carry into register 23 and show which tests go red by name; then wrap at the wrong power of two and show what catches it. (b) Restore from a **committed** baseline, never `git checkout --` over uncommitted work. (c) Actually red: a mutation applied and still green means the runner is not executing your patch — cargo fingerprints by mtime, so watch for "Compiling". (d) Wired into `cargo test --workspace`, expectations **derived** from the ROM's tables and the documented rule rather than copied from a neighbouring pin, and loud when something cannot be measured — never render "couldn't measure" as 0 or green. (e) If you tighten the method midway, re-establish the earlier claims under it and say which. **For every assertion, ask: if this went green for a reason OTHER than the claim holding, what would that reason be?** Name it and say how you ruled it out.

## Running and verifying

- **One cargo invocation at a time in your worktree.** Targeted runs while you work (`cargo test -p oracle-core --test conformance_roms vdp_port_access`, `cargo test -p oracle-core --lib`). A test that fails only under machine load is a defect with a narrow window, not a flake. Report it by name.
- Final check: **`./tools/land.sh --no-push`** from your worktree root after your last commit, on a clean tree. **Detach it** per its header: a run-unique wrapper in YOUR scratch dir that writes `LAND-EXIT=$? AT $(date -Is)` to your log, started with `setsid nohup … &`. **Then poll your own log in the FOREGROUND until LAND-EXIT appears**: `for i in $(seq 1 7); do grep -q '^LAND-EXIT=' "$LOG" && break; sleep 15; done`, repeated. **Do NOT end your turn while it runs, and never wait on a background-task notification** — it may never reach you. Never run land.sh in the foreground. Do not commit while it runs. A killed or capped run is never a verdict.
- Baseline, **release profile**: **89/89 legs, 2884 passed / 0 failed / 3 ignored**. **Predict your leg and pass counts before the run** and report the prediction beside the result. A moved leg count is an explanation or a problem — find out which.
- Report aggregate totals with the profile, never a tail. Grep failures with `^test [^ ]+ \.\.\. FAILED`. Capture exit codes outside pipes. Match processes by exact name (`pgrep -x cargo`).
- Commit as each piece lands, **with the findings in the message**, so a death costs the run and never the work. Write messages from a file and read them back with `git log -1 --format=%B`. Exact-path `git add` (never `-A`), `git show --stat` after each commit. No `Co-Authored-By` trailer.

## Report shape

1. Where this brief was wrong. 2. Branch, tip SHA (from git output, never typed), commits. 3. The wrap arithmetic derived from the ROM, and your call on sharing `advance_dma_source_low16` with reasons. 4. The ROM before and after: tally, failing set, prediction against result, and which flips are joint with A1 rather than yours alone. 5. Tests added, each with its mutation (line on disk, the named failure, the restore) and the alternative green path ruled out. 6. Frozen currency: every golden and currency constant, byte-identical or explained. 7. `land.sh --no-push`: verdict token, LAND-EXIT, predicted and observed legs, passed/failed/ignored, profile. 8. Open items, TAGs, anything BLOCKED.
