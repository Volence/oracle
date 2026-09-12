# VDP-PORT-ACCESS-FULL-ROM: the dispatched brief

> **Dispatch artifact, 2026-09-12 (overseer, session 9).** The board's `next` row, booked by M22's landing
> (`docs/2026-09-11-lens-triage.md`, the "Updated 2026-09-12 after wave 1's two measure-first rows" paragraph,
> residue item 2). Taken under the owner's standing "If something stops we have it work on the next item"
> (empyrean `origin/main` `9ef70cf`, `docs/OVERSEER.md` l.35) and the hub's rebooted-lane line (same file, l.135).
> Premises measured at oracle `40edd0b`: `scrape_vdp_port_access` (`crates/oracle-core/tests/conformance_roms.rs`
> ~l.613) reads the ROM's printed `Results:` tally after page 1 and after page 2 only, and its golden row (~l.228-229)
> is `page1 pass/fail/total=9/0/9; pages1+2 cumulative=16/0/16`. `vdp_port_access_copy_dma_matches_the_roms_own_tables`
> (~l.930) already drives the WHOLE ROM to 122 tests by pressing `Start` whenever the tally is still for 120 frames,
> decodes every result record with `port_access_records` (~l.857), and asserts only the 28 copy records (test 26,
> tests 96-122, last eight words of each matrix record). M22's agent measured the whole ROM at **76 passed / 46
> failed / 122** after its fix. So ~106 tests we vendor have results nobody reads. Baseline, release profile,
> `tools/land.sh --no-push` at `bd60c3d` (only notes since): **89/89 legs, 2877 passed / 0 failed / 3 ignored**.
> **This parcel changes no emulation code**, so no frozen golden can move. The text below the rule is the agent
> prompt, passed with `isolation: worktree`.

---

You are a measurement agent in the **oracle** repo (a from-scratch Rust Mega Drive emulator and its Aether debug bus). Parcel: **VDP-PORT-ACCESS-FULL-ROM**, size M, one branch. **You read, sort and pin. You do not fix emulation.**

**First, tell me where this brief is wrong.** Every mechanism and count below is a hypothesis. Your command output outranks it.

## The question

The vendored hardware test ROM `vdp_port_access` (VDPFIFOTesting) runs **122** VDP port-access tests over 22 pages. Each test stores a result record in 68000 RAM from `$FF0000`: its title, and (when the ROM has one) its expected-value table beside what the VDP answered. Our scorecard reads only the first 16. The whole ROM reportedly fails 46. **For each failing test: what hardware behaviour does it check, why does our VDP give a different answer, and what kind of problem is that?**

## What to do

1. **Inventory all 122.** Reuse the existing paging loop and `port_access_records`. Factor them into shared helpers rather than copying them, and do not boot the ROM more times than the tests need. For every record, capture: test number, page, title, whether it has an expected table, the ROM's own verdict (`expected == actual`, which the existing test cross-checks against the ROM's printed pass count), and for failures the expected and actual words. Records with no expected table: find out from the ROM how it judges them (read its code; the display routine is at `$0D8A`, and the existing doc comment gives the record layout) and say so.
2. **Sort every failure into exactly one bucket, with evidence.**
   - **A, behaviour:** our VDP returns a wrong VALUE, independent of timing. Name the mechanism in our code (`file:line`) and the hardware rule the table implies. Each A is a candidate fix row for me.
   - **T, timing:** the answer depends on when the access lands (FIFO depth, access slots, DMA timing, the busy flag, HV counter). Name what the timing is.
   - **M, unmodelled:** the test exercises something we do not model at all (e.g. the post-copy data-port reads the copy-matrix records' first eight words hold; test 29, the copy's source register). Name what is missing.
   - **U, unexplained:** say so plainly. U is an honest answer, not a failure of the parcel.
   Say how you decided each one. **Group failures that share a cause**; the useful count is causes, not tests. If one cause explains many failures, show the evidence that it is ONE cause (what in the tables varies, and what stays the same).
3. **Pin the full verdict set** as a conformance test wired into `cargo test --workspace`: the set of failing tests **by number and title**, so any flip, a fix or a regression, fails naming the test that moved. Derive it from the ROM's own verdicts, cross-checked against its printed tally, never typed from your inventory. Decide what happens to the old two-page golden row (keep it, widen it, or retire it for the new test) and say why. The owner's standing rule governs how this pin is read: **timing unknowns may be deferred, behaviour unknowns get pinned from a reference and fixed, never waved through as expected failures.** So the pinned failing set is a record of today, and every A-bucket cause becomes its own queue row (your report proposes them). It must not read as an accepted-failure list. Say that in the test's doc comment.
4. **Write it up** in `docs/2026-09-12-vdp-port-access-full-rom.md`: the 122-row inventory (a table), the bucketed failures with evidence, the proposed rows (one per A cause, with size and the file each would touch), and what evidence would settle each U. Add a dated line to the `vdp_port_access` entry in `docs/2026-07-25-testrom-conformance.md` pointing at it. Never rewrite history in a dated doc.

## Clean-room: binding

`crates/oracle-core/src/vdp.rs`'s module doc says: "no emulator source informs this code (clean-room, audit policy 3)". **Do not open, fetch, search or quote any emulator's source in any form.** That covers BlastEm, Genesis Plus GX, Ares, jgenesis, Exodus, MAME, and this workspace's own `oracle-old` C++ core, including GitHub code search and blame views.
**Allowed evidence:** the test ROM itself (its code and tables; disassemble it from `vendor/`), hardware documentation (Sega manuals, Nemesis's *VDP Internals*, Charles MacDonald's VDP notes, Plutiedev), and forum PROSE from hardware testers (SpritesMind threads). You may use WebFetch/WebSearch for those docs and threads, never for code. The ROM's author may have published notes or a readme. Look for them.

## 0. Base check, branch, vendor, scratch

- `cd` to your worktree root (absolute path) before any git operation.
- Base check: `git merge-base --is-ancestor 40edd0b HEAD && echo OK` must print OK and `docs/2026-09-12-vdp-port-access-full-rom-brief.md` must exist. If the file is missing, run `git fetch origin && git merge --ff-only origin/main` once and re-check. If it is still missing, you are BLOCKED: stop.
- `git switch -c parcel/vdp-port-access-full-rom`. Record your tip SHA as you go. **The controller (me) merges your branch into main and then deletes the branch and your worktree**, so a missing branch afterwards is expected. Check with `git merge-base --is-ancestor <tip> main` first. If that says no, confirm by content, because a squash merge rewrites commits.
- `ln -s /home/volence/sonic_hacks/oracle/vendor vendor` (gitignored). Without it the ROM tests print SKIP and pass, which is a silent false green. **Confirm the full-ROM test actually ran by its output, not by `ok`.** `ls` is aliased to eza, so use `command ls`.
- Scratch: a run-unique dir you create, `/tmp/claude-1000/vdp-port-access-$(date +%s)/`.
- **If `origin/main` moves while you work**, `tools/land.sh` refuses at G3. Then run `git fetch origin && git merge origin/main` into your branch (never rebase) and re-run.

## Hard boundaries

- **Files in scope:** `crates/oracle-core/tests/conformance_roms.rs`, any test helper or example you add under `crates/oracle-core/`, `docs/2026-09-12-vdp-port-access-full-rom.md` (new), `docs/2026-07-25-testrom-conformance.md` (a dated line only). **Nothing under `crates/*/src/`**: no emulation change. If you find a fix you are sure of, write it into the proposed rows with the evidence. Do not make it.
- **Never touch** `docs/lane-status.json`, `docs/lane-log.jsonl`, `docs/decisions.jsonl`, `docs/lens-findings.jsonl` or any `docs/OVERSEER*.md`, nor the vendored contract/schema.
- **No emulator, ever.** Never call any `mcp__oracle__*` tool. Never launch a window or BlastEm on any display. You drive the ROM headless through `oracle_core::System` in a test, exactly as the existing tests do. TAG anything that needs a live look.
- **BLOCKED is always available**: stop that item, record why, finish the rest. Never degrade the design to reach green.
- Do not merge, push, or open a PR.

## Every check you add follows these five rules

(a) **Red-first, SHOWING THE MUTATION APPLIED**: quote the mutated line from disk, or `git diff --stat`, before the red run. Natural mutations: flip one entry in your pinned failing set and show the test fails naming that test; break the paging (stop pressing `Start`) and show it fails loudly rather than checking fewer tests. (b) Restore from a **committed** baseline, never `git checkout --` over uncommitted work. (c) Actually red: a mutation that is applied and still green means the runner is not executing your patch (cargo fingerprints by mtime, so watch for "Compiling"). (d) Wired into `cargo test --workspace`, expectations **derived** from the ROM's own verdicts, and loud on unmeasurable: a ROM that never reaches 122, or a record count that disagrees with the ROM's tally, is a failure, never a smaller check. (e) If you tighten the method midway, re-establish earlier claims under it and say which ones. **For every assertion, ask: if this went green for a reason OTHER than the claim holding, what would that reason be?** Name the reason in the report and say how you ruled it out.

## Running and verifying

- **One cargo invocation at a time in your worktree.** Use targeted runs while you work (`cargo test -p oracle-core --test conformance_roms vdp_port_access`). A test that fails only under machine load is a defect with a narrow window, not a flake: report it by name.
- **Measure the debug-profile runtime** of every test that boots this ROM (CI runs debug). Report wall time per test in both profiles, with the machine's load average at the time.
- Final check: **`./tools/land.sh --no-push`** from your worktree root after your last commit, on a clean tree. **Detach it** per its header: a run-unique wrapper in YOUR scratch dir that writes `LAND-EXIT=$? AT $(date -Is)` to your log, started with `setsid nohup … &`. **Then poll your own log in the FOREGROUND until LAND-EXIT appears**: `for i in $(seq 1 7); do grep -q '^LAND-EXIT=' "$LOG" && break; sleep 15; done`, repeated. **Do NOT end your turn while it runs, and never wait on a background-task notification.** Never run land.sh in the foreground. Do not commit while it runs.
- Baseline, **release profile**, `tools/land.sh --no-push` at `bd60c3d`: **89/89 legs, 2877 passed / 0 failed / 3 ignored**. **Predict your leg and pass counts before the run** and report prediction against result. A moved leg count needs an explanation, or it is a problem, so say which. Every frozen golden and currency constant must come out byte-identical. This parcel touches no emulation, so any movement is a defect in your change.
- Report aggregate totals with the profile, never a tail. Grep failures with `^test [^ ]+ \.\.\. FAILED`. Capture exit codes outside pipes. Match processes by exact name (`pgrep -x cargo`).
- Commit as each piece lands, **with the findings in the message**, so a death costs the run and never the work. Write messages from a file and read them back with `git log -1 --format=%B`. Use exact-path `git add` (never `-A`) and run `git show --stat` after each commit. No `Co-Authored-By`.

## Report shape

1. Where this brief was wrong. 2. Branch, tip SHA (from git), commits. 3. The tally re-measured (passed/failed/total, and how you know every record was read). 4. The failures by cause: bucket, tests, evidence, and the code or hardware rule. 5. Proposed queue rows, one per A cause, plus what would settle each U. 6. The pin: test name, what it asserts, each mutation (line on disk, then the named failure, then the restore), and the alternative green path ruled out. 7. What happened to the two-page golden row and why. 8. Runtimes, debug and release, with load average. 9. `land.sh --no-push`: verdict token, LAND-EXIT, predicted and observed legs, passed/failed/ignored, profile. 10. Open items, TAGs, anything BLOCKED.
