# VSRAM-DECODE (cause A1): the dispatched brief

> **Dispatch artifact, 2026-09-12 (overseer, session 10).** First fix of the board row VDP-PORT-ACCESS-FIXES,
> cause A1 of `docs/2026-09-12-vdp-port-access-full-rom.md`. Taken under the owner's standing "If something stops
> we have it work on the next item" (empyrean `origin/main` `db47b94`, `docs/OVERSEER.md` l.35) and the hub's
> rebooted-lane line (same file, l.135). Dispatched in parallel with DMA-SRC-ADVANCE (cause A3,
> `docs/2026-09-12-dma-src-advance-brief.md`). Both move bytes, so they **land one after the other**, never
> together. Premises: no file under `crates/*/src` changed between the write-up's tip `dd4defe` and `965ab1a`.
> Baseline, release profile, `tools/land.sh --no-push` at `0f8d62e` (only notes since): **89/89 legs, 2878 passed /
> 0 failed / 3 ignored**. The text below the rule is the agent prompt.

---

You are an implementation agent in the **oracle** repo (a from-scratch Rust Mega Drive emulator and its Aether debug bus). Parcel: **VSRAM-DECODE**, size M, one branch, **one byte-moving fix**.

**First, tell me where this brief is wrong.** Every mechanism, line number and count below is a hypothesis. Your command output outranks it.

## The fix

Read `docs/2026-09-12-vdp-port-access-full-rom.md` in full first: sections "A1", "Proposed queue rows", "What would settle the open points" and "The pin". The rule, from hardware testers' prose quoted there: **the VSRAM address is 7 bits, so it wraps at `$80`. Writes to `$50-$7F` are discarded. Reads from `$50-$7F` return the VSRAM read latch.** Ours wraps at 80 bytes (`% VSRAM_SIZE`) in `read_target` and `write_target` (`crates/oracle-core/src/vdp.rs`). **Both halves change together**: test 13 goes red under either half alone.

**Enumerate every path that decodes a VSRAM address, by what TOUCHES VSRAM storage, not by the two functions named above.** Include port writes, port reads, 68k-to-VSRAM DMA, fill into VSRAM, any copy path, the renderer's vertical-scroll fetch, snapshot and restore, and any debug or bus surface that reads or writes VSRAM directly. For each one, say whether the hardware decode applies and what you did. A debug client's direct VSRAM read or write is not the data port. Leave its addressing alone unless you find a reason, and report what you found.

## The design call you own: the read latch

The write-up's open point: every `$50-$7F` read in the ROM's tables returns `$0123`, and in each of those tests VSRAM words 0 and 1 both hold `$0123`, so the ROM cannot tell "word 0", "word 1" and "the renderer's last vertical-scroll fetch" apart. The documented mechanism (Nemesis, quoted in the doc) is an internal register that latches VSRAM read data. The doc argues it is **not** the port's own pre-read: reading word 39 just before `$50` would leave `$0560`, and the table says `$0123`. The owner's standing rule is that **behaviour unknowns get pinned from a reference and fixed, never waved through**. So model the documented mechanism rather than the scratch shortcut "return word 0". If you find the mechanism cannot be modelled cleanly, say why and what you chose instead, and name what evidence would overturn the choice.

If you add machine state, these constraints bind:

- **The snapshot carries it**, so a restore is exact. Check the VDP snapshot and restore path, `Vdp::check_regions`, and the bincode layout.
- **`export_state` and `state_hash` are wire-visible.** Their byte layouts are pinned by the Aether contract. If you conclude the latch must enter either of them, **STOP that item and report**. That is a contract change and needs a change request, never a drive-by edit. Say whether leaving it out of them is sound, and why.
- **Masked renders never write state.** They take `&self`. The committed-sprite-latch test `masked_renders_leave_the_committed_sprite_latches_untouched` and `docs/2026-08-26-layer-mask.md` are the record. If the renderer feeds the latch, it does so only on the committed path. **Every run path that commits a scanline must feed it the same way**, including `oracle-replay`'s cheap unmasked path, or two runs of one input diverge. Enumerate those paths by what calls the commit, and show the list.

## What must flip and what must not

Predicted from the write-up's scratch experiment (row EXP=3, both A1 halves): the ROM goes from **76/46/122 to 112/10/122**, failing exactly **20 27 28 29 31 32 33 34 36 38**. So tests **23, 74 77 80 83 86 89 92 95, and 96-122** flip to pass, **test 13 stays green**, and test 20 stays failing (it also needs cause A2). Predict before you run, and report the prediction beside the result.

- **Re-derive `PORT_ACCESS_FAILING` from the run**, never type it from this brief. The same goes for the scorecard row's `all 22 pages cumulative=` tally.
- Update the unit test the write-up names (`vdp::tests::a_register_write_retains_cd5_cd2`, which indexes VSRAM storage with the old decode). Change only what the decode change forces, and say what.
- Add direct unit tests for the decode: a write to `$50-$7F` is discarded, an address at or above `$80` maps by `& $7F`, and a read of `$50-$7F` returns the latch. Each is red-first (rules below).
- **Frozen currency: predicted byte-identical.** Under all five causes' rules together, the scratch experiment moved no frozen golden (`determinism_gate`, `export_state_v1`, `golden_frames`, `scanline_goldens`), and every scorecard row except `vdp_port_access` stayed byte-identical. A latch in the snapshot can still move a snapshot-derived value. Any movement is either explained or a defect, so say which.
- Docs: append a dated line to the `vdp_port_access` entry in `docs/2026-07-25-testrom-conformance.md`, and a dated "Fixed" note under "A1" in `docs/2026-09-12-vdp-port-access-full-rom.md`. Append only. Never rewrite history in a dated doc.

## A second agent is working beside you

DMA-SRC-ADVANCE (cause A3) is being fixed at the same time on another branch. It changes `run_fill` and `run_copy` in `vdp.rs` to advance source registers 21 and 22, and it too updates `PORT_ACCESS_FAILING` and the scorecard tally. **Do not touch source-register handling in `run_fill` or `run_copy`.** The fixes land one at a time. Whichever lands second will be asked to merge `main` and re-derive its pin, so expect that message and do not pre-empt it.

## Clean-room: binding

`crates/oracle-core/src/vdp.rs`'s module doc says: "no emulator source informs this code (clean-room, audit policy 3)". **Do not open, fetch, search or quote any emulator's source in any form.** That covers BlastEm, Genesis Plus GX, Ares, jgenesis, Exodus, MAME and this workspace's own `oracle-old` C++ core, including GitHub code search and blame views. **Allowed evidence:** the test ROM (disassemble it from `vendor/`), hardware documentation, and forum prose by hardware testers. WebFetch and WebSearch are for those docs and threads, never for code.

## 0. Base check, branch, vendor, scratch

- `cd` to your worktree root (absolute path) before any git operation.
- Base check: `git merge-base --is-ancestor 965ab1a HEAD && echo OK` must print OK, and `docs/2026-09-12-vsram-decode-brief.md` must exist. If the file is missing, run `git fetch origin && git merge --ff-only origin/main` once and check again. If it is still missing, you are BLOCKED: stop.
- `git switch -c parcel/vsram-decode`. Record your tip SHA as you go. **The controller (me) merges your branch into main and then deletes the branch and your worktree**, so a missing branch afterwards is expected. Check with `git merge-base --is-ancestor <tip> main` first. If that says no, confirm by content, because a squash merge rewrites commits.
- `ln -s /home/volence/sonic_hacks/oracle/vendor vendor` (gitignored). Without it the ROM tests print SKIP and pass, which is a silent false green. **Confirm the full-ROM test actually ran by its output, not by `ok`.** `ls` is aliased to eza, so use `command ls`.
- Scratch: a run-unique dir you create, `/tmp/claude-1000/vsram-decode-$(date +%s)/`.
- **If `origin/main` moves while you work**, `tools/land.sh` refuses at G3. Then run `git fetch origin && git merge origin/main` into your branch (never rebase) and re-run.

## Hard boundaries

- **Files in scope:** `crates/oracle-core/src/vdp.rs`; any other `oracle-core` file your enumeration shows the fix needs (say why, per file); `crates/oracle-core/tests/conformance_roms.rs`; the two docs named above. Anything outside `oracle-core`: ask first by stopping that item and reporting.
- **Never touch** `docs/lane-status.json`, `docs/lane-log.jsonl`, `docs/decisions.jsonl`, `docs/lens-findings.jsonl`, any `docs/OVERSEER*.md`, or the vendored contract or schema.
- **No emulator, ever.** Never call any `mcp__oracle__*` tool. Never launch a window or BlastEm on any display. Drive the ROM headless through `oracle_core::System` in tests, as the existing tests do. TAG anything that needs a live look.
- **BLOCKED is always available**: stop that item, record why, finish the rest. Never degrade the design to reach green.
- Do not merge, push, or open a PR.

## Every check you add follows these five rules

(a) **Red-first, SHOWING THE MUTATION APPLIED**: quote the mutated line from disk, or `git diff --stat`, before the red run. The natural mutation is to revert one decode half at a time and show which tests go red, by name. (b) Restore from a **committed** baseline, never with `git checkout --` over uncommitted work. (c) Actually red: a mutation that is applied and still green means the runner is not executing your patch. Cargo fingerprints by mtime, so watch for "Compiling". (d) Wired into `cargo test --workspace`, with expectations **derived** from the ROM's own verdicts and from the rule, and loud when something cannot be measured. (e) If you tighten the method midway, re-establish the earlier claims under it and say which ones. **For every assertion, ask: if this went green for a reason OTHER than the claim holding, what would that reason be?** Name it in the report and say how you ruled it out.

## Running and verifying

- **One cargo invocation at a time in your worktree.** Use targeted runs while you work (`cargo test -p oracle-core --test conformance_roms vdp_port_access`, `cargo test -p oracle-core --lib vdp`). A test that fails only under machine load is a defect with a narrow window, not a flake. Report it by name.
- Final check: **`./tools/land.sh --no-push`** from your worktree root after your last commit, on a clean tree. **Detach it** per its header: a run-unique wrapper in YOUR scratch dir that writes `LAND-EXIT=$? AT $(date -Is)` to your log, started with `setsid nohup … &`. **Then poll your own log in the FOREGROUND until LAND-EXIT appears**: `for i in $(seq 1 7); do grep -q '^LAND-EXIT=' "$LOG" && break; sleep 15; done`, repeated. **Do NOT end your turn while it runs, and never wait on a background-task notification.** Never run land.sh in the foreground. Do not commit while it runs.
- Baseline, **release profile**, `tools/land.sh --no-push` at `0f8d62e`: **89/89 legs, 2878 passed / 0 failed / 3 ignored**. **Predict your leg and pass counts before the run**, and report the prediction beside the result. A moved leg count needs an explanation, or it is a problem, so say which.
- Report aggregate totals with the profile, never a tail. Grep failures with `^test [^ ]+ \.\.\. FAILED`. Capture exit codes outside pipes. Match processes by exact name (`pgrep -x cargo`).
- Commit as each piece lands, **with the findings in the message**, so a death costs the run and never the work. Write messages from a file and read them back with `git log -1 --format=%B`. Use exact-path `git add` (never `-A`) and run `git show --stat` after each commit. No `Co-Authored-By`.

## Report shape

1. Where this brief was wrong. 2. Branch, tip SHA (from git), commits. 3. The VSRAM path enumeration, a verdict per path. 4. The latch: what you modelled, why, where it lives (snapshot, `export_state`, `state_hash`), the committing paths that feed it, and what would overturn it. 5. The ROM before and after: tally, failing set, prediction against result. 6. Tests added, each with its mutation (line on disk, then the named failure, then the restore) and the alternative green path ruled out. 7. Frozen currency: every golden and currency constant, byte-identical or explained. 8. `land.sh --no-push`: verdict token, LAND-EXIT, predicted and observed legs, passed/failed/ignored, profile. 9. Open items, TAGs, anything BLOCKED.
