# M24 parcel 3: the Z80 `EI` delay (the dispatched brief)

> **Dispatch artifact, 2026-09-14 (overseer, session 21).** The go is the hub's (empyrean-52, by message ~00:05Z),
> given under the owner's goodnight delegation, read at empyrean `f13feff:docs/OVERSEER.md:99`: *"if you need any
> decisions you don't think I need to answwer feel free to confer yourself"*. `f13feff` is an ancestor of their
> `origin/main`. It is a log-tick commit, so it is the revision the words were read at, not the one that carried them.
> The order is this repo's `docs/OVERSEER.md` "Order of work, 2026-09-13" (delete B landed `f0f0a96`; NEXT parcel 3,
> no ruling needed: the design's §7 row 3, "the documentation is unambiguous").
> Premises measured at oracle `0355ed7`. Nothing under `crates/`, `tools/`, `examples/`, `Cargo.toml` or `Cargo.lock`
> changed after the delete-B merge `f0f0a96`, so the delete-B agent's run stands as the baseline: **release profile,
> `tools/land.sh --no-push`: 90/90 legs, 2906 passed / 0 failed / 3 ignored.**
> The text below the rule is the agent prompt, passed with `isolation: worktree`.

---

You are an implementation agent in the **oracle** repo, a from-scratch Rust Mega Drive emulator and its Aether debug bus. One small parcel: **the Z80 `EI` delay**. When `EI` executes, a pending maskable interrupt must not be accepted until the instruction *after* `EI` has run. Today the core accepts at the very next instruction boundary.

**Authority:** `docs/2026-09-13-z80-timing-currency-design.md`. Read its plain summary, §0, §2.1, §2.2, §2.4, §6.1 and §7 (row 3, "How the currency verifies", "Regeneration") before writing anything. Then read the whole header of `crates/oracle-core/tests/z80_timing_probes.rs`. Its table is the current statement of what this parcel must move.

**First, tell me where this brief is wrong.** Every claim below is a hypothesis, and your command output outranks it. Each of the last several agents in this repo corrected its brief on material points, measured. Do the same here, and put it first in your report.

## The fix

1. **The shadow.** `EI` is `0xFB` in `Z80::execute` (`crates/oracle-core/src/z80/mod.rs` ~972). Acceptance is sampled in `Z80::step` (~584, `if self.int_pending && self.iff1`). Make the instruction after `EI` complete before any acceptance. The shape is yours (a one-instruction shadow flag is the obvious one). Name every path that must set, honour or clear it: `EI` itself, the acceptance, `DI`, the reset (ZC9's reset model, `mod.rs` ~356-410), the latched-fault path at the top of `step`, and `HALT`.
2. **Two edge cases the manual is silent on (§6.1, "Silent").** These are open design calls with defensible answers. Make them, cite your reasoning, and pin each with a unit test.
   - **A run of `EI`s.** Does each `EI` re-arm the delay? My prior is yes: the `EI` page's own note is "during the execution of this instruction and the following instruction, maskable interrupts are disabled", and the following instruction here is itself an `EI`. That is a prior, not a finding.
   - **`EI; HALT`.** `HALT` is the instruction after `EI`, so the shadow covers `HALT`'s own execution and acceptance comes at the first idle boundary after it. §2.4 says `system::tests::z80_takes_the_vblank_interrupt_and_runs_its_im1_handler` (`EI; HALT`) stayed green under the spike's `ei` mutation. Confirm that it still does, and say whether its timing moved.
3. **A bus grant that cuts the instruction after `EI`** (M21's machinery, `system.rs`). Establish from the code whether the shadow can be consumed or lost across a cut. If a grant can end the shadow before the following instruction has finished executing, that is a defect in the fix.

## Machine state: enumerate before you add a field

The shadow is machine state for exactly one instruction, and a frame boundary, a checkpoint or a save can fall inside it. **Enumerate every place Z80 state is copied, compared, hashed or serialized. Derive the list by grep over the whole workspace, never from this list.** Candidates: `Clone`; `Z80::export_region` and `System::export_state` region 4 (30 live bytes padded to the reserved `0x40`, `mod.rs` ~315 and `system.rs` ~1143-1212); `state_hash`; any save/load or checkpoint format (the `oracle-frontend` `save_state` rows); the flat register view (`Z80Regs`, `from_regs`, `mod.rs` ~177-231) and whatever serves it on the Aether wire. For each one, say whether the shadow must go in and why.

- The design says the shadow fits in region 4's reserve as a content fill at unchanged size, with no version bump, and that the export golden's Z80 is held in reset, so it stays zero. **Verify all three claims.**
- **STOP and report BLOCKED** (naming the path) if carrying the shadow would change any of these: a frozen layout, a pinned golden other than the probes, a save-format version, or an Aether wire reply or schema. Those need a ruling, and a wire change is a contract change request. Never leave the shadow out of a save/restore path to avoid a ruling, either. A save taken between `EI` and the next instruction that restores with the shadow lost is a determinism defect. If you find that case, report it as a finding.

## An external oracle, if the corpus has one

The SST-z80 gate checks only the final IFF bits (design §2.4). **Check whether the vendored SingleStepTests Z80 corpus records an `EI`-just-executed field (or similar) in its initial/final states.** If it does, grade your flag against it. That gives an expected value from outside this emulator. Report the case count and any disagreements. If it does not, say so, and state what you looked at.

## The pins: move exactly what the table names

Per the probes header and design §7 row 3, this parcel must move:
- **C1** `A` from 0 to 1;
- **C2a** `HL` from 0 to 1;
- **C2b** `HL` from 0 to 1 ("Parcel 3 alone moves HL to 1").

It must **not** move C2a's or C2b's `V`, **C3** (it guards M21), or **C4**'s count (parcel 4's). **Write your prediction down before you run the probes.** If a predicted pin fails to move, or an unnamed one moves, stop and report. That is a finding, and re-pinning to green would hide it. Each moved constant changes only in the commit that changes the behaviour, next to a `cause:` line in the design's form (`cause: M24 EI delay (UM0080 p.18): C1 A 0 -> 1`, with the measured delta). Update the probes header table's "Pinned today" column, and remove each **WRONG** marker the fix discharges. Leave C2b's `V` WRONG, since that is parcel 4's. Leave the design doc itself untouched.

**Then sweep for what RESTATES the old behaviour** (a standing lesson in this repo: a claim repaired at its canonical site leaves its paraphrases standing). Vary the spelling and the axis: "sampled at the instruction boundary", "at each instruction boundary", "re-arms only when the driver `EI`s", and doc comments on `step`, `accept_interrupt`, `set_int_line`, the `iff1` field and `System`'s VInt path. Report each phrasing searched, each hit, and whether you fixed or cleared it.

## 0. Base check, branch, vendor, scratch

- `cd` to your worktree root (absolute path) before any git operation.
- Base check: `git merge-base --is-ancestor 0355ed7 HEAD && echo OK` must print OK, and `docs/2026-09-14-m24-parcel-3-ei-delay-brief.md` must exist. If the file is missing, run `git fetch origin && git merge --ff-only origin/main` once and check again. If it is still missing, report BLOCKED and stop.
- `git switch -c parcel/m24-ei-delay`. Record your tip SHA as you go. **I (the controller) merge your branch into main and then delete the branch and your worktree**, so a missing branch afterwards is the expected end state. Check `git merge-base --is-ancestor <tip> main` first. If that says no, confirm by content, because a squash merge rewrites commits.
- `ln -s /home/volence/sonic_hacks/oracle/vendor vendor` (gitignored), then check that `vendor/` resolves (17 TestRoms entries). Without it, 8 `save_state` rows fail and the 68000 SST sweep silently skips. `ls` is aliased to eza, so use `command ls`.
- Scratch: create your own run-unique directory, `/tmp/claude-1000/m24-ei-$(date +%s)/`. Put nothing of yours in any shared scratch path.

## Hard boundaries

- **Files in scope:** `crates/oracle-core/src/z80/mod.rs`; the probes file (pins, `cause:` lines, header table); new unit tests; the serialization sites your enumeration shows need the flag; the paraphrase sites your sweep finds. For anything else, say why in your report before touching it.
- **Never touch** `docs/lane-status.json`, `docs/lane-log.jsonl`, `docs/decisions.jsonl`, `docs/lens-findings.jsonl`, any `docs/OVERSEER*.md`, the design doc, or the vendored contract/schema. Queue outcomes go in your report, and I transcribe them.
- **No parcel 4** (no `/INT` width or level change, and acceptance keeps consuming `int_pending`). **No parcel 5** (no Z80 access hook).
- **No emulator, ever.** Never call any `mcp__oracle__*` tool, and never launch a window on any display. TAG anything that needs a live run or a listen.
- **BLOCKED is always available:** stop that item, record why, and finish the rest. Never degrade a design to reach green.
- Do not merge, push, or open a PR.

## Every gate you add follows these rules

- **(a) Red-first, with the mutation shown applied** before the red run: quote the mutated line from disk, or show `git diff --stat` naming the file.
- **(b) Restore from a committed baseline**, never with `git checkout --` over uncommitted work.
- **(c) A mutation that is applied but still green is a finding about the instrument first.** Cargo fingerprints by mtime, so watch for "Compiling", and fix the runner before claiming the gate.
- **(d) Wire the gate into a runner that executes it**, and name that runner. Derive expected values from UM0080 and the clock constants, never by reading them back from the implementation. Be loud when something can't be measured; never report a silent 0.
- **(e) If you tighten a method midway,** re-establish the earlier claims under it and say which ones.
- **For every "it moved" or "it did not move", ask what else would produce the same result,** and say how you ruled it out.
- **Vary the mutation parameter.** At minimum:
  - the shadow never set (C1 back to 0);
  - the shadow never cleared (acceptance starved: which rows catch it?);
  - the shadow cleared one instruction early;
  - the shadow dropped from the save/restore path, if you added it there.

## Running and verifying

- **Run one cargo invocation at a time in your worktree.** No other agent is running cargo in this repo. Use targeted runs while you work. Every timing figure ships with `cat /proc/loadavg` and the wall-clock uptime, because peers share this machine.
- **A known intermittent failure exists and is not yours:** `crates/oracle-aether/tests/machine_replaced.rs:291`, "expected exactly one emulator/machineReplaced in the stream, got 0". Its race was fixed 09-13 (`7ccd42d`). If it appears again, report it by name as a new sighting and do not chase it. Any other failure that only appears under load is a defect with a narrow window, not a flake: report it by name.
- **Final check, after your last commit, on a clean tree:** run `./tools/land.sh --no-push` from your worktree root.
  - **Detach it** as its header describes: a run-unique wrapper in YOUR scratch directory writes `LAND-EXIT=$? AT $(date -Is)` to your log, started with `setsid nohup … &`.
  - **Then wait on your own log in the FOREGROUND:** `timeout 540 tail -F "$LOG" | grep -m1 '^LAND-EXIT='`, repeated until it prints. This environment refuses `for … sleep` poll loops.
  - **Do NOT end your turn while it runs, and never wait on a background-task notification.** Never run land.sh in the foreground, and don't commit while it runs.
  - The script lives at `tools/land.sh`. Read the `LAND-EXIT` token in the log, never a notification's exit code.
- **Baseline** (release profile, `tools/land.sh --no-push`): **90/90 legs, 2906 passed / 0 failed / 3 ignored.** Predict your leg count and your passed/ignored counts before the run, then report prediction against result, with the profile named on every count.
- **Reporting:** aggregate totals, never a tail. Grep failures with `^test [^ ]+ \.\.\. FAILED`, capture exit codes outside pipes, and match processes by exact name (`pgrep -x cargo`).
- **Commits:** commit as each piece lands, with **findings in the message**, so a death costs the run and never the work.
  - Write messages to a file and commit with `-F <file>` (never `-F -`, and never backquotes inside `$(…)`). Read each one back with `git log -1 --format=%B`.
  - Use exact-path `git add` (never `-A`) and run `git show --stat` after each commit.
  - No `Co-Authored-By`.

## Report shape

1. Where this brief was wrong.
2. Branch, tip SHA (from git), the commits, and `git diff --stat 0355ed7...<tip>`.
3. The fix: the flag's shape; every path that sets, honours or clears it; the two edge-case calls, with reasons and the tests that pin them; the bus-grant finding.
4. The state enumeration: each site, whether the shadow is in it, and why; the region-4, no-version-bump and export-golden claims, each verified or refuted.
5. The SST corpus check.
6. The pins: prediction against result for C1, C2a, C2b, C3 and C4, with the `cause:` lines as committed.
7. The paraphrase sweep: phrasings, hits, verdicts.
8. The red-first proofs and the mutation table, with on-disk evidence.
9. `land.sh --no-push`: verdict token, LAND-EXIT, predicted and observed legs, passed/failed/ignored, profile.
10. Open items, TAGs, and anything BLOCKED.
