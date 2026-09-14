# M1-FILL-OVER-TIME: the dispatched brief (design doc first)

> **Dispatch artifact, 2026-09-14 (overseer, session 22).** Go: the hub, under the owner's goodnight delegation,
> by message at ~01:05Z. Order read at empyrean `3d322af:docs/OVERSEER.md:105` ("M1-FILL-OVER-TIME after those"),
> delegation verbatim at `:99`; `3d322af` verified an ancestor of empyrean `origin/main`. It is a status-tick
> commit, so it is where the words can be READ, not the commit that carries them. **The go stops at the doc.**
> How saves and the frozen currencies move goes back to the hub before anything is built, and to the owner if
> it changes what an existing save does.
> Premises measured at oracle `14cb815`. `git diff --stat d421f51 14cb815 -- crates tools Cargo.toml Cargo.lock`
> is empty, so the parcel-3 landing run is the baseline: **release profile, `tools/land.sh --no-push` on the
> merged tree of `d421f51`: 90/90 legs, 2912 passed / 0 failed / 3 ignored.**
> The text below the rule is the agent prompt, passed with `isolation: worktree`.

---

You are a design agent in the **oracle** repo (a from-scratch Rust Mega Drive emulator and its Aether debug bus). Parcel: **M1-FILL-OVER-TIME**, size L, one branch. **The deliverable is a design document, not a fix.** You may write measurement spikes to answer its questions. Label them `spike:` and remove them from the tip before you report (the H22/M24 pattern: kept in history, gone from the tip, so the merge is docs-only).

**First, tell me where this brief is wrong.** Every mechanism below is a hypothesis, and your command output outranks it. The last six agents in this repo each corrected their brief on material points, with measurements. Do the same here.

## Why this parcel exists

1. **The last three failures of the VDPFIFOTesting ROM.** It now scores 119/3/122, and the three failures are tests 31, 32 and 33. The failing set is pinned as `PORT_ACCESS_FAILING` in `crates/oracle-core/tests/conformance_roms.rs`. All three share one cause: on hardware a DMA fill takes time, and a data-port write made during it changes the rest of the fill. Read `docs/2026-09-12-vdp-port-access-full-rom.md` §"M1" (about line 395) and §"Proposed queue rows" first. The ROM's evidence (test 31 at ROM `$2BD8`): it fills `$FFB` bytes with `$12`, waits, and writes `$5678` mid-fill. Hardware reads the tail at `$8FF8` as `5656 5656 0056`; we read `1212 1212 5678`. Tests 32 and 33 are the CRAM and VSRAM forms.
2. **Ours finishes instantly.** `MegaDriveBus::run_pending_dma` (`crates/oracle-core/src/bus.rs`) calls `Vdp::run_fill` (`crates/oracle-core/src/vdp.rs`), which writes every byte inside the trigger write and then only opens a busy window (`dma_busy_until`). Since A4 (lane log `2026-09-12T21:12:11Z`), `dma_busy()` is `fill_armed() || mclk < dma_busy_until`, where `fill_armed` is **derived** from existing state rather than stored. That derive-don't-store design is deliberate, so keep it where you can. Find every site by symbol; line numbers in older docs are stale.
3. **Why it was deferred, and why that is not a timing question.** The write-up classifies M1 as a missing mechanism, not a timing error. The ROM reads the fill's tail, so where exactly the switch happens does not change the tables. What is missing is a fill that exists across time and shares the FIFO. Timing matters only once that exists: the per-slot rate then decides how far the fill has got. It borders two deferred items: the "Phase 3 per-line DMA cost" in `docs/2026-08-03-a3-dma-fifo-design.md`, and **F-DMAHALT** (`docs/2026-07-25-testrom-conformance.md`, about line 1170). F-DMAHALT warns that changing DMA elapsed time moves every DMA-using ROM, which makes it a real currency risk.

## What the design doc must answer

Write `docs/2026-09-14-m1-fill-over-time-design.md`. Put a plain summary first, so a reader who stops there knows the recommendation, its cost, and **what it does to existing saves**. Then answer the sections below.

1. **The hardware behaviour, from documentation only.** Answer each with a quote and its source: how a fill advances (per access slot? at what rate in active display versus blanking, H32 versus H40?); what a data-port write during a fill does (it goes through the FIFO; the fill then resumes with a byte of the new word, so say *which* byte, and derive it against test 31's `5656 5656 0056` and group 3's `9a9a 9a9a 9a9a`); how the CRAM and VSRAM forms differ (tests 32/33, `0666`/`0888`); whether the 68000 stalls; what the status bits read during a fill (DMA busy, FIFO empty/full); and what a control-port write or a data-port read does mid-fill. Also say whether **VRAM copy** (`run_copy`) runs over time the same way, and whether it belongs in this design or a later one, with a cause. **Clean-room: no emulator source in any form** (BlastEm, Genesis Plus GX, Ares, jgenesis, MAME, Exodus, and this workspace's `oracle-old` C++ core). Allowed: Nemesis's *VDP Internals*, SpritesMind threads, Plutiedev, Sega documentation, and the VDPFIFOTesting ROM's own code, disassembled as the 2026-08-03 and 2026-09-12 docs did. Where the documentation disagrees or is silent, say so and do not pick a number.
2. **The state model.** Say what machine state a fill in progress needs (for example remaining length, current address, fill data, a DMA clock or slot position) and how it joins the existing FIFO model (`fifo`, `fifo_len`, `fifo_slot_clock`, `fifo_drain`), the A3 source-register advance, A4's derived busy flag, and A5's no-target fill (`code_names_a_write_target`). **Better-approach pass, required:** weigh at least (a) lazy catch-up, where the fill advances to "now" at each VDP access, line render and frame end; (b) eager per-slot or per-line advancement from the scheduler (`system.rs`'s event list); (c) whatever you find better. Pick one, say why, and say what the losers would have done better. Prefer designs where busy and progress are derived from one piece of state, so the two cannot disagree.
3. **The renderer.** A fill that crosses active display changes VRAM mid-frame. Say what `render_scanline` and the scanline latches must see: does catch-up have to run before each line renders, and what does that cost on the hot path? The null/no-fill path must stay as cheap as it is now. Say how you keep it that way, and measure it if a spike touches it.
4. **SAVES AND FROZEN CURRENCIES. This section is what comes back for a ruling before anything is built.** Enumerate each of the following, and for each say *moves / does not move*, measured by a spike where you can, with a named mechanism:
   - the `export_state` v1 layout (`docs/export-state-v1.md`: is there reserve room, or does it need a version bump?);
   - `state_hash` (`crates/oracle-core/src/state_hash.rs`: does it cover the new state?);
   - the save-state container's derived machine-layout fingerprint (`oracle_frontend::save_state`; `crates/oracle-player/src/states.rs` names it). A layout change there makes **every older save refuse to load**, as happened at `28e4587` and again at `d421f51`;
   - snapshot/bincode;
   - the Aether wire (does any client-visible field change: status bits, a DMA field?);
   - every frozen golden: visual baselines, scanline goldens, the aeon replay fixtures, vendored-ROM scorecards, the 22 printed VDPFIFOTesting tallies, and `PORT_ACCESS_FAILING`.

   Answer plainly: **does any save that loads today load differently, or refuse, after this?** What does a save taken mid-fill contain, and what happens when it is restored? A wire-visible change is a contract change request, which is a ruling for me, not a design choice for you.
5. **Which tests move.** 31/32/33 flip to pass. Predict their exact tail words from the mechanism, derived and not read back. Say which of the other 119 must not move; the fill tests 4, 16, 28, 29, 34, 36 and 38 especially. List which existing unit tests encode instant completion (for example `vdpfifo_t4_fill_trigger_and_byte_placement` and the busy-window tests in `bus.rs`/`vdp.rs`) and would need a `cause:` line if they move. This repo's rule: goldens never regenerate silently, and every mover carries a named, measured mechanism.
6. **Who else changes behaviour.** Measure with a spike on the committed corpus (aeon replay fixtures, vendored ROMs): how many fills run, their lengths, and how many are followed by a VRAM/CRAM/VSRAM access, a status poll or a DMA **before** the new model says the fill completes. That population is exactly the set whose behaviour changes. Say whether any replay verdict is at risk, and **TAG** anything that needs a live look.
7. **Staging.** List the parcels in order, what each moves (bytes, saves, frozen currency, wire), and which need a ruling. If the work splits so that a first parcel moves no save layout, say so; that is worth a lot.

## 0. Base check, branch, vendor, scratch

- `cd` to your worktree root (absolute path) before any git operation.
- Base check: `git merge-base --is-ancestor 14cb815 HEAD && echo OK` must print OK, and `docs/2026-09-14-m1-fill-over-time-design-brief.md` must exist. If it is missing, run `git fetch origin && git merge --ff-only origin/main` once and re-check. Still missing means BLOCKED: stop.
- `git switch -c parcel/m1-fill-over-time-design`. Record your tip SHA as you go. **The controller (me) merges your branch into main and then deletes the branch and your worktree**, so a missing branch afterwards is expected. Check `git merge-base --is-ancestor <tip> main` first, and confirm by content if that says no (a squash merge rewrites commits).
- `ln -s /home/volence/sonic_hacks/oracle/vendor vendor` (it is gitignored), then check that `vendor/` resolves (17 TestRoms entries). Without it, 8 `save_state` rows fail, the 68000 SST sweep silently skips, and VDPFIFOTesting does not run. `ls` is aliased to eza, so use `command ls`.
- Scratch: a run-unique dir you create, `/tmp/claude-1000/m1-fill-design-$(date +%s)/`. Nothing of yours goes in any shared scratch path.

## Hard boundaries

- **Files in scope:** the new design doc, plus spike code anywhere under `crates/` or `examples/`. Commit spikes as `spike:` and **remove them from the tip in a final `spike:` commit**, so that `git diff --stat 14cb815...<tip>` names docs only. Anything else: say why in the report before you touch it.
- **Never touch** `docs/lane-status.json`, `docs/lane-log.jsonl`, `docs/decisions.jsonl`, `docs/lens-findings.jsonl` or any `docs/OVERSEER*.md`, nor the vendored contract/schema. Queue outcomes go in your report, and I transcribe them.
- **No fix on this branch**, not even a small one. Design it.
- **No emulator, ever.** Never call any `mcp__oracle__*` tool, and never launch a window on any display. TAG anything that needs a live run or a look.
- **BLOCKED is always available**: stop that item, record why, and finish the rest. Never degrade a design to reach a tidy answer.
- Do not merge, push, or open a PR.

## Every measurement and check follows these rules

(a) **Show a spike's mutation applied** before its run: quote the mutated line from disk, or show `git diff --stat`.

(b) Restore from a **committed** baseline, never with `git checkout --` over uncommitted work.

(c) **Applied and unmoved is a finding about the instrument first.** Cargo fingerprints by mtime, so watch for "Compiling". An instrument that cannot move is not evidence of stillness.

(d) **Derive** expected values from documentation and constants (the slot tables, `MCLK_PER_LINE`, the ROM's own tables), never by reading them back from the implementation. Be loud on unmeasurable, never a silent 0.

(e) If you tighten a method midway, re-establish the earlier claims under it and say which.

**For every "it moved" or "it did not move", ask what else would produce the same result** (a corpus with no mid-fill access, a golden that excludes the field, a fill too short to cross a line), and say how you ruled it out.

## Running and verifying

- **One cargo invocation at a time in your worktree.** No other agent is running cargo in this repo right now. Use targeted runs while you work. Every timing figure ships with the machine's load average (`cat /proc/loadavg`) and wall-clock uptime, since peers share this box.
- No intermittent failure is currently known in this repo: the `machine_replaced` race and the player first-read race both landed fixes. **Any failure is reported by name**, and one that only appears under load is a defect with a narrow window, not a flake.
- Final check, after your last commit (the spike-removal one), on a clean tree: **`./tools/land.sh --no-push`** from your worktree root.
  - **Detach it** per its header: a run-unique wrapper in YOUR scratch dir that writes `LAND-EXIT=$? AT $(date -Is)` to your log, launched with `setsid nohup … &`.
  - **Then poll your own log in the FOREGROUND until LAND-EXIT appears**, one bounded wait per tool call: `timeout 110 tail -F "$LOG" | grep -m1 '^LAND-EXIT='`, repeated until it prints. Poll loops of the `for … sleep` form are refused by this environment.
  - **Do NOT end your turn while it runs, and never wait on a background-task notification.** Never run land.sh in the foreground. Do not commit while it runs.
  - The script is `tools/land.sh`. Read the `LAND-EXIT` token in the log, never a notification's exit code.
- Baseline (release profile, `tools/land.sh --no-push`): **90/90 legs, 2912 passed / 0 failed / 3 ignored.** With a docs-only net diff you should reproduce it exactly. Predict before the run, and report prediction against result.
- Report aggregate totals with the profile, never a tail. Grep failures with `^test [^ ]+ \.\.\. FAILED`. Capture exit codes outside pipes. Match processes by exact name (`pgrep -x cargo`).
- Commit as each piece lands, **with findings in the message** (a death must cost the run, never the work).
  - Write messages to a file and commit with `-F <file>`. Never `-F -`, and never backquotes inside `$(…)`. Read each back with `git log -1 --format=%B`.
  - Use exact-path `git add` (never `-A`) and run `git show --stat` after each commit. No `Co-Authored-By`.

## Report shape

1. Where this brief was wrong.
2. Branch, tip SHA (from git), commits (spike commits named as such), and `git diff --stat 14cb815...<tip>`.
3. The recommendation in two sentences, plus one plain sentence on what it does to existing saves.
4. The hardware answers, quoted with sources, and where documentation is silent.
5. The state model and the better-approach table.
6. The saves/currency table (section 4), each row moved / not moved, with its on-disk proof.
7. Tests that move, with predicted tail words for 31/32/33, and the population measurement from section 6.
8. Proposed parcels in order, with what each moves and which need a ruling.
9. `land.sh --no-push`: verdict token, LAND-EXIT, predicted and observed legs, passed/failed/ignored, profile.
10. Open items, TAGs, and anything BLOCKED.
