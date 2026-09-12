# FILL-BUSY-ARM + FILL-TGT (causes A4 and A5): the dispatched brief

> **Dispatch artifact, 2026-09-12 (overseer, session 11).** Fourth and fifth fixes of the board row
> VDP-PORT-ACCESS-FIXES, causes A4 and A5 of `docs/2026-09-12-vdp-port-access-full-rom.md`. Taken under the
> owner's standing 2026-09-11T18:23:49Z instruction and the hub's rebooted-lane line, both verified
> firsthand at empyrean `origin/main` `8fe1122`, `docs/OVERSEER.md` l.73-80 and l.128-135. Dispatched in
> parallel with DMA-SRC-128K (cause A2, `docs/2026-09-12-dma-src-128k-brief.md`). Both move bytes, so they
> **land one after the other**, never together. **Two fixes on one branch, deliberately** — both live in the
> fill path and splitting them across agents would put two agents in the same functions. They get **one
> commit each with its own ROM measurement**, so A/B attribution survives the pairing. Premises: A1
> (VSRAM-DECODE) and A3 (DMA-SRC-ADVANCE) are already on `main` at `d34f37c`; the ROM stands at
> **114/8/122**, failing **20 27 31 32 33 34 36 38** (read from `PORT_ACCESS_FAILING` at `d34f37c`, not from
> the write-up). Baseline, release profile, `tools/land.sh --no-push` on the A1 merge: **89/89 legs, 2884
> passed / 0 failed / 3 ignored**. Both CI runs for `e3a4456` and `d34f37c` are SUCCESS on all three jobs.
> The text below the rule is the agent prompt.

---

You are an implementation agent in the **oracle** repo (a from-scratch Rust Mega Drive emulator and its Aether debug bus). Parcel: **FILL-BUSY-ARM + FILL-TGT**, two size-S byte-moving fixes, one branch, **one commit each**.

**First, tell me where this brief is wrong.** Every mechanism, line number and count below is a hypothesis. Your command output outranks it. The last three agents on this board row each corrected their brief on at least one point; that is the expected outcome, not a failure.

## Why these two are paired

Both are in the DMA-fill path and would otherwise put two agents inside the same functions. **They are still two separate fixes and must stay separately attributable**: commit A4 with its own ROM tally in the message, then A5 with its own. Do not blend them into one commit, and do not let the second one's measurement stand in for both.

Read `docs/2026-09-12-vdp-port-access-full-rom.md` first: sections "A3", "A4", "A5", "M1", "What would settle the open points" and "The pin".

## A4 — DMA busy is set by the fill command itself (tests 36, 38)

**The rule.** Eke, *VDP Internals* p.4: "on DMA Fill, busy flag is actually immediately (?) set after the CTRL port write, not the DATA port write that starts the Fill operation". The ROM confirms it.

**Ours.** Status bit 1 is `mclk < dma_busy_until` (`vdp.rs:660`). Only a *completed* transfer opens that window (`run_fill` `vdp.rs:1440`, `run_copy` `vdp.rs:1487`, `dma_complete`). The fill's control word only arms (`vdp.rs:1092`, `arm_dma` `vdp.rs:1099`).

**Evidence.** **Test 36** (ROM `$B6F8`) reads status after the fill command `$40020082` and before the `$1234` trigger. Hardware gives `$0202` (FIFO empty plus busy); ours gives `$0200`. **Test 38** (ROM `$C34C`) group 2 is the same, and busy stays set across a `$8144` register write and a `$4002` half-command (`$C870..$C878`). In group 1, where DMA is **disabled** when the command is written, neither machine reports busy — so the arm is conditional on DMA being enabled, and that negative case is part of the rule, not a detail. This is behaviour, not timing: both samples fall between two port writes and depend on nothing the beam does.

**The open question this fix carries, and it is yours to answer with reasons, not to block on.** The ROM does not say **what ends the busy flag of a fill that is armed but never triggered**. Test 38 shows only that it survives a register write and a half-command. Pick the most defensible model, implement it, and **write down in the report what would distinguish your choice from the alternatives, and what a ROM that settled it would have to do** — the write-up's "What would settle the open points" already names the shape (arm a fill, never trigger it, poll status across a new command word and across frames). If you find the choice changes any currently-passing test, that is a finding: say so before you pick.

Predicted: EXP=16 flips exactly **36 and 38**. Tests **35, 37 and 39-41 must not move.**

## A5 — a fill whose code names no write target writes nothing (test 34, closes F-FILLTGT)

**The rule, from the ROM's own table.** **Test 34** (ROM `$45B2`) group 3 (`$4898..$4948`) arms a 4-byte fill with `$40020082`, then writes register `$8F02`, which leaves code `$22` (a register write replaces CD1-CD0 with `10`, pinned by test 13). Then comes the `$68AC` trigger. On hardware VRAM `$8000-$800F` reads back **unchanged**; ours shows `5568 7768 9968 bb68`.

**Ours.** The trigger write is already suppressed by `code_names_a_write_target`, but the fill *body* resolves its target through `target_of`'s `_ => Vram` fallback (`vdp.rs:714`, used at `vdp.rs:1395`) and writes VRAM. **The fill body must share the write decode.**

**⚑ The half that is easy to get wrong, and the reason this is not simply "skip the fill".** The hardware fill still **ran**. Group 4 writes no length and fills well past 16 bytes, which means group 3 had counted its length down to 0 (that is, 65,536). So a fill with no write target still **consumes its length, still advances the DMA source registers 21/22 (the `advance_dma_source_low16` that A3 landed at the end of `run_fill` last session), and still opens its busy window** — it just writes nowhere. A fix that returns early from `run_fill` would pass test 34 and silently regress A3's tests 28/29. **Verify that 28 and 29 still pass and say so explicitly in your report.**

This closes the booked follow-up **F-FILLTGT**, whose original text said "one of the two decodes is wrong; the ROM does not cover the case" — test 34 does cover it. Retire it in the docs you touch.

Predicted: EXP=32 flips exactly **34**. Tests **4 and 72-95 must not move.**

## What must flip and what must not, together

Predicted from the write-up's scratch matrix (row EXP=59, "all but A2", against the current state): the ROM goes from **114/8/122 to 117/5/122**, failing **20 27 31 32 33**. Tests 20 and 27 are A2's, being fixed on the other branch right now — they stay failing for you. **Tests 31, 32 and 33 are M1 (FILL-OVER-TIME), a size-L design item that is deliberately out of scope**: a fill that runs across time and shares the FIFO with port writes. Do not attempt it, and do not let either of your fixes drift toward it.

- **Re-derive `PORT_ACCESS_FAILING` from the run**, never type it from this brief. Same for the scorecard row's `all 22 pages cumulative=` tally.
- Add a direct unit test per fix: busy reads set between the fill command and its trigger (and clear in the DMA-disabled case); and a fill whose code names no write target leaves memory unchanged **while still advancing 21/22 and consuming its length**. Red-first, rules below.
- **Frozen currency: predicted byte-identical.** Under all five causes together the scratch experiment moved no frozen golden (`determinism_gate`, `export_state_v1`, `golden_frames`, `scanline_goldens`) and every scorecard row except `vdp_port_access` stayed byte-identical. The busy window is machine state a game polls, so movement is possible; any movement is either explained or a defect, and you say which.
- Docs: append a dated line to the `vdp_port_access` entry in `docs/2026-07-25-testrom-conformance.md`, and dated "Fixed" notes under **"A4"** and **"A5"** in `docs/2026-09-12-vdp-port-access-full-rom.md`, plus the F-FILLTGT retirement. **Append only. Never rewrite history in a dated doc.**

## A second agent is working beside you

DMA-SRC-128K (cause A2) is being fixed at the same time on another branch. It changes the **68k DMA source path** — `run_mem_dma` in `bus.rs` and `dma_complete` in `vdp.rs` — so that the source wraps inside its 128 KB page and never carries into register 23. You share `vdp.rs`. **Do not touch `run_mem_dma`, `dma_complete`, or register 23 handling.** You both re-pin `PORT_ACCESS_FAILING` and both append to the same two docs, so a conflict there is expected and is mine to resolve. Whichever lands second will be asked to merge `main` and re-derive its pin and tally; expect that message and do not pre-empt it.

## Clean-room: binding

`crates/oracle-core/src/vdp.rs`'s module doc says: "no emulator source informs this code (clean-room, audit policy 3)". **Do not open, fetch, search or quote any emulator's source in any form.** That covers BlastEm, Genesis Plus GX, Ares, jgenesis, Exodus, MAME and this workspace's own `oracle-old` C++ core, including GitHub code search and blame views. **Allowed evidence:** the test ROM (disassemble it from `vendor/`), hardware documentation, and forum prose by hardware testers. WebFetch and WebSearch are for those docs and threads, never for code.

## 0. Base check, branch, vendor, scratch

- `cd` to your worktree root (absolute path) before any git operation.
- Base check: `git merge-base --is-ancestor d34f37c HEAD && echo OK` must print OK, and `docs/2026-09-12-fill-busy-and-target-brief.md` must exist. If the file is missing, run `git fetch origin && git merge --ff-only origin/main` once and check again. If it is still missing, you are BLOCKED: stop.
- `git switch -c parcel/fill-busy-and-target`. Record your tip SHA as you go. **The controller (me) merges your branch into main and then deletes the branch and your worktree**, so a missing branch afterwards is the expected end state. Check with `git merge-base --is-ancestor <tip> main` first — and note that a non-ancestor result does **not** prove you were reaped, because a squash or rebase rewrites commits. Confirm by content.
- `ln -s /home/volence/sonic_hacks/oracle/vendor vendor` (gitignored). Without it the ROM tests print SKIP and pass, which is a silent false green. **Confirm the full-ROM test actually ran by its output, not by `ok`.** `ls` is aliased to eza, so use `command ls`.
- Scratch: a run-unique dir you create, `/tmp/claude-1000/fill-busy-and-target-$(date +%s)/`.
- **If `origin/main` moves while you work**, `tools/land.sh` refuses at G3. Then `git fetch origin && git merge origin/main` into your branch (never rebase) and re-run.

## Hard boundaries

- **Files in scope:** `crates/oracle-core/src/vdp.rs` (the fill path, the busy window, `target_of`), `crates/oracle-core/tests/conformance_roms.rs`, and the two docs named above. `bus.rs` only if your enumeration shows a completion path there that you must change — say why, and remember the other agent is editing its DMA source path. Anything else: stop that item and report.
- **Never touch** `docs/lane-status.json`, `docs/lane-log.jsonl`, `docs/decisions.jsonl`, `docs/lens-findings.jsonl`, any `docs/OVERSEER*.md`, or the vendored contract or schema.
- **No emulator, ever.** Never call any `mcp__oracle__*` tool. Never launch a window or BlastEm on any display. Drive the ROM headless through `oracle_core::System` in tests, as the existing tests do. TAG anything that needs a live look for my foreground follow-up.
- **BLOCKED is always available**: stop that item, record why, finish the rest. Never degrade the design to reach green. In particular: if A4's open question turns out to force a worse design, stop on A4, report, and land A5 alone.
- Do not merge, push, or open a PR.

## Every check you add follows these five rules

(a) **Red-first, SHOWING THE MUTATION APPLIED**: quote the mutated line from disk, or `git diff --stat`, before the red run. Natural mutations: move the busy arm back to completion only, and show which tests go red by name; arm it unconditionally so the DMA-disabled case breaks; restore `target_of`'s `_ => Vram` fallback in the fill body; and make the no-target fill return early so 28/29 break. (b) Restore from a **committed** baseline, never `git checkout --` over uncommitted work. (c) Actually red: a mutation applied and still green means the runner is not executing your patch — cargo fingerprints by mtime, so watch for "Compiling". (d) Wired into `cargo test --workspace`, expectations **derived** from the ROM's tables and the documented rule rather than copied from a neighbouring pin, and loud when something cannot be measured — never render "couldn't measure" as 0 or green. (e) If you tighten the method midway, re-establish the earlier claims under it and say which. **For every assertion, ask: if this went green for a reason OTHER than the claim holding, what would that reason be?** Name it and say how you ruled it out.

## Running and verifying

- **One cargo invocation at a time in your worktree.** Targeted runs while you work (`cargo test -p oracle-core --test conformance_roms vdp_port_access`, `cargo test -p oracle-core --lib vdp`). A test that fails only under machine load is a defect with a narrow window, not a flake. Report it by name.
- Final check: **`./tools/land.sh --no-push`** from your worktree root after your last commit, on a clean tree. **Detach it** per its header: a run-unique wrapper in YOUR scratch dir that writes `LAND-EXIT=$? AT $(date -Is)` to your log, started with `setsid nohup … &`. **Then poll your own log in the FOREGROUND until LAND-EXIT appears**: `for i in $(seq 1 7); do grep -q '^LAND-EXIT=' "$LOG" && break; sleep 15; done`, repeated. **Do NOT end your turn while it runs, and never wait on a background-task notification** — it may never reach you. Never run land.sh in the foreground. Do not commit while it runs. A killed or capped run is never a verdict.
- Baseline, **release profile**: **89/89 legs, 2884 passed / 0 failed / 3 ignored**. **Predict your leg and pass counts before the run** and report the prediction beside the result. A moved leg count is an explanation or a problem — find out which.
- Report aggregate totals with the profile, never a tail. Grep failures with `^test [^ ]+ \.\.\. FAILED`. Capture exit codes outside pipes. Match processes by exact name (`pgrep -x cargo`).
- Commit as each piece lands, **with the findings in the message**, so a death costs the run and never the work. Write messages from a file and read them back with `git log -1 --format=%B`. Exact-path `git add` (never `-A`), `git show --stat` after each commit. No `Co-Authored-By` trailer.

## Report shape

1. Where this brief was wrong. 2. Branch, tip SHA (from git output, never typed), commits — **A4 and A5 separately**. 3. A4: your model for what ends an untriggered fill's busy flag, the alternatives, and what would distinguish them. 4. A5: proof that the no-target fill still consumes its length and still advances 21/22, with tests 28 and 29 named as still passing. 5. The ROM after each commit separately: tally, failing set, prediction against result. 6. Tests added, each with its mutation (line on disk, the named failure, the restore) and the alternative green path ruled out. 7. Frozen currency: every golden and currency constant, byte-identical or explained. 8. `land.sh --no-push`: verdict token, LAND-EXIT, predicted and observed legs, passed/failed/ignored, profile. 9. Open items, TAGs, F-FILLTGT's retirement, anything BLOCKED.
