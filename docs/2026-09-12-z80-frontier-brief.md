# Z80-FRONTIER (lens M21): the dispatched brief

> **Dispatch artifact, 2026-09-12 (overseer, session 8).** LENS-WAVE-1's CORE-EMULATION-TIMING, first row only:
> M21. M24 (EI delay, `/INT` width) and M5 (`export_state_hash` FM/PSG placeholders) share `system.rs` with it
> and are separate byte-movers, so they wait for this one to land. Runs beside `parcel/copy-dma-xor` (lens M22,
> `vdp.rs` only); the two share no file and land one after the other.
> Premises measured at oracle `9c5185f`: `System::catch_up_z80` (`crates/oracle-core/src/system.rs:1793-1833`).
> Gated on (`z80_running && !z80_busreq`), it runs whole instructions `while *z80_frontier_mclk < now`, so it can
> stop PAST `now` by up to one instruction (the ledger's figure is up to 345 mclk, i.e. 23 T-states × 15 mclk).
> Gated off, `self.z80_frontier_mclk = now;` runs unconditionally, which rolls that overshoot BACK: a gate that
> closes and re-opens lets the Z80 re-run time it has already spent. The function's own doc says no committed
> fixture releases the Z80, so the corpus never takes the gated-on branch; that is a claim to measure, not to
> take. Baseline, release profile, `tools/land.sh --no-push` at `efdd1a6` (only a lane-log line since): 89/89
> legs, 2867 passed / 0 failed / 3 ignored. The text below the rule is the agent prompt, passed with
> `isolation: worktree`.

---

You are an implementation agent in the **oracle** repo (a from-scratch Rust Mega Drive emulator and its Aether debug bus). Parcel: **Z80-FRONTIER** (lens finding M21), size S, one branch. **Reproduce first, then fix.**

**First, tell me where this brief is wrong.** Every mechanism below is a hypothesis; your command output outranks it.

## The defect

`System::catch_up_z80` (`crates/oracle-core/src/system.rs`) keeps the Z80's absolute clock, `z80_frontier_mclk`, on the same timeline as the 68000. While the Z80 is gated on it runs whole instructions until its frontier reaches or passes the 68000's `now`, carrying the overshoot forward, which is the right pattern. While it is gated off (held in reset, or the bus granted to the 68000) the frontier is set to `now` **unconditionally**, so any overshoot left by the last gated-on step is refunded. A game that toggles BUSREQ often, as sound drivers do, gives its Z80 up to one extra instruction's worth of time per toggle.

## What to do

1. **Reproduce, red at HEAD.** A test that runs a Z80 loop while the 68000 side opens and closes the gate, and counts the Z80 T-states actually executed against the gated-on time that elapsed. Derive the expected bound from `MCLK_PER_Z80_CYCLE` and the gate intervals (the executed time may exceed the gated-on time by at most one instruction, **in total, not per toggle**). Show it red at HEAD with the refund measured.
2. **The two ways the gate closes are different hardware events. Rule on each, with the reasoning written into the code.** Evidence is documentation only (Zilog's Z80 user manual on BUSREQ/BUSACK and RESET, Sega's hardware manuals, Plutiedev, SpritesMind prose): **clean-room, no emulator source in any form** (BlastEm, Genesis Plus GX, Ares, jgenesis, MAME, and this workspace's `oracle-old` C++ core). Questions to answer, not answers to transcribe:
   - **Bus granted:** the Z80 finishes what it was doing before it releases the bus. Here that means the overshoot is time really spent, so the frontier must not move back.
   - **Reset asserted:** an instruction in flight is abandoned. When reset is released at `T`, does the Z80 start at `T`, or wait until the old frontier if that is later? `max(frontier, now)` answers "wait", which may be wrong for reset.
   - **Better-approach pass, required:** `max()` is the obvious fix. Say whether it is the best one, or whether the design should change (for example, one owner for "the Z80 stopped at instant X"). Pick one and say why.
3. **Enumerate by what TOUCHES `z80_frontier_mclk`, not by this function.** Every reader and writer: snapshot/restore, `export_state`, `state_hash`, the FM timer read at the frontier, the VDP port mirror, reset, the run loop's drain ordering (the ORDERING HAZARD comment), any profiler or debug surface. For each, say whether "frontier may now exceed `now` while gated off" breaks an assumption it makes.
4. **Measure what moves.** Across every committed ROM, fixture and frozen currency the suite runs (state hashes, `export_state` goldens, visual baselines, the aeon fixtures, replay, scanline goldens): does any of them take the gated-on branch? Use an instrument (a counter in a test or example), never the doc's claim. **If any frozen currency moves, stop and report BLOCKED with the measurement: that is a ruling for me, not an edit for you.** If none moves, say how you know the instrument could have seen one (a positive control).
5. **Fix, test, sync.** The fix, the item-1 test turned green, a test per gate-close cause pinning your item-2 ruling, and the function's doc rewritten so it states the invariant, including every paraphrase of "the frontier is advanced to `now`" (vary the spelling: `z80_frontier`, "frontier", "zero backlog", `ZC5`) across `crates/` and the Z80 design docs (dated docs get a dated line, never a rewrite). Append one line to `docs/lens-findings.jsonl` for M21 (`"state":"fixed"`, `fixedAt` your fix commit), copying the shape of existing lines.

## 0. Base check, branch, vendor, scratch

- `cd` to your worktree root (absolute path) before any git operation.
- Base check: `git merge-base --is-ancestor 9c5185f HEAD && echo OK` must print OK and `docs/2026-09-12-z80-frontier-brief.md` must exist. If the file is missing, run `git fetch origin && git merge --ff-only origin/main` once and re-check; still missing means BLOCKED, stop.
- `git switch -c parcel/z80-frontier`. Record your tip SHA as you go. **The controller (me) merges your branch into main and then deletes the branch and your worktree**; a missing branch afterwards is expected. Check `git merge-base --is-ancestor <tip> main` first, and confirm by content if that says no (a squash merge rewrites commits).
- `ln -s /home/volence/sonic_hacks/oracle/vendor vendor` (gitignored). `ls` is aliased to eza, so use `command ls`.
- Scratch: a run-unique dir you create, `/tmp/claude-1000/z80-frontier-$(date +%s)/`.
- **If `origin/main` moves while you work**, `tools/land.sh` refuses at G3. Then `git fetch origin && git merge origin/main` into your branch (never rebase) and re-run.

## Hard boundaries

- **Files in scope:** `crates/oracle-core/src/system.rs`, `crates/oracle-core/src/z80/` if the ruling needs it, tests/examples you add, the Z80 design docs under `docs/` (a dated line only), `docs/lens-findings.jsonl` (append only). **Not `crates/oracle-core/src/vdp.rs`**: another agent owns it right now. **Not M24 or M5**: note anything you see about them and leave it. Anything else: say why in the report before touching it.
- **Never touch** `docs/lane-status.json`, `docs/lane-log.jsonl`, `docs/decisions.jsonl` or any `docs/OVERSEER*.md`, nor the vendored contract/schema.
- **No snapshot, `export_state` or `state_hash` layout change.**
- **No emulator, ever.** Never call any `mcp__oracle__*` tool; never launch a window on any display. TAG anything that needs a live run.
- **BLOCKED is always available**: stop that item, record why, finish the rest. Never degrade a design to reach green.
- Do not merge, push, or open a PR.

## Every check you add follows these five rules

(a) **red-first, SHOWING THE MUTATION APPLIED** (quote the mutated line from disk or `git diff --stat` before the red run). The natural mutation is restoring `self.z80_frontier_mclk = now;`, and the item-1 test must then fail naming the refunded amount. (b) Restore from a **committed** baseline, never `git checkout --` over uncommitted work. (c) Actually red: applied-and-still-green means the runner isn't executing your patch (cargo fingerprints by mtime; watch for "Compiling"). (d) Wired into `cargo test --workspace`, expectations **derived** from `MCLK_PER_Z80_CYCLE` and the instruction timings, never read back from the implementation; loud on unmeasurable. (e) If you tighten the method midway, re-establish earlier claims under it and say which. **For every assertion, ask: if this went green for a reason OTHER than the rule holding, what would that reason be?** (For example, a Z80 loop whose every instruction ends exactly on `now` leaves no overshoot to refund, so the test passes at HEAD. Choose the instruction mix so an overshoot is certain, and assert that it happened.) Name the reason in the report and say how you ruled it out.

## Running and verifying

- **One cargo invocation at a time in your worktree.** Targeted runs (`cargo test -p oracle-core --lib system::`) while you work. A test that fails only under machine load is a defect with a narrow window, not a flake: report it by name.
- Final check: **`./tools/land.sh --no-push`** from your worktree root after your last commit, clean tree. **Detach it** per its header (run-unique wrapper in YOUR scratch dir writing `LAND-EXIT=$? AT $(date -Is)` to your log; `setsid nohup … &`), **then poll your own log in the FOREGROUND until LAND-EXIT appears**: `for i in $(seq 1 7); do grep -q '^LAND-EXIT=' "$LOG" && break; sleep 15; done`, repeated. **Do NOT end your turn while it runs, and never wait on a background-task notification.** Never foreground land.sh. Do not commit while it runs.
- Baseline, **release profile**, `tools/land.sh --no-push` at `efdd1a6`: **89/89 legs, 2867 passed / 0 failed / 3 ignored**. **Predict your leg and pass counts before the run** and report prediction vs result; a moved leg count is an explanation or a problem, so say which.
- Report aggregate totals with the profile, never a tail; grep failures with `^test [^ ]+ \.\.\. FAILED`; capture exit codes outside pipes; match processes by exact name (`pgrep -x cargo`).
- Commit as each piece lands, **findings in the message** (a death must cost the run, never the work); messages from a file, read back with `git log -1 --format=%B`. Exact-path `git add` (never `-A`), `git show --stat` after each. No `Co-Authored-By`.

## Report shape

1. Where this brief was wrong. 2. Branch, tip SHA (from git), commits. 3. The reproduction: the test, the refund measured at HEAD. 4. The ruling per gate-close cause, with quoted documentation. 5. The enumeration of what touches `z80_frontier_mclk` and the verdict per site. 6. The corpus measurement and its positive control. 7. Tests by name, each mutation (line on disk → named failure → restore), the alternative green path ruled out. 8. Surfaces changed (code, docs, paraphrases, ledger line). 9. `land.sh --no-push`: verdict token, LAND-EXIT, predicted vs observed legs, passed/failed/ignored, profile. 10. Open items (including anything seen about M24/M5), TAGs, anything BLOCKED.
