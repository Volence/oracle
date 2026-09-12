# COPY-DMA-XOR (lens M22, follow-up F-COPYXOR): the dispatched brief

> **Dispatch artifact, 2026-09-12 (overseer, session 8).** LENS-WAVE-1's CORE-VDP remainder: M22, the one
> measure-first row in `vdp.rs`. Runs beside `parcel/z80-frontier` (lens M21), which touches
> `crates/oracle-core/src/system.rs` only; the two share no file, and each moves emulation bytes, so they land
> one after the other, never in one branch.
> Premises measured at oracle `9c5185f`: `Vdp::run_copy` (`crates/oracle-core/src/vdp.rs:1447-1458`) reads
> `self.vram[src & (VRAM_SIZE - 1)]` and writes `self.addr as usize & (VRAM_SIZE - 1)`, no `^ 1` on either side.
> The fill engine (`vdp.rs:1405-1414`) writes at `address ^ 1`, citing Eke (SpritesMind, *Is DMA Fill buggy?*):
> "VRAM byte writes (used by VRAM fill **and copy** DMA) actually occur to VRAM address ^ 1"; its comment says
> `run_copy` was left alone because no vendored test covers it. The fill half of that sentence is pinned by
> VDPFIFOTesting test 4 (`vram_fill_writes_the_msb_to_address_xor_one`). The A3 design declined the copy half
> (`docs/2026-08-03-a3-dma-fifo-design.md:609-615`, Q2: "pinned from a ROM or an instrument, not from this citation
> alone"; the copy's source READ is "also unresolved") and registered follow-up `F-COPYXOR` in
> `docs/2026-07-25-testrom-conformance.md`. **Correction to the triage:** `docs/2026-09-11-lens-triage.md:208-209`
> says to settle this "from BlastEm's `vdp.c`". That is barred here (clean-room, below). Baseline, release profile,
> `tools/land.sh --no-push` at `efdd1a6` (only a lane-log line since): 89/89 legs, 2867 passed / 0 failed / 3
> ignored. The text below the rule is the agent prompt, passed with `isolation: worktree`.

---

You are an implementation agent in the **oracle** repo (a from-scratch Rust Mega Drive emulator and its Aether debug bus). Parcel: **COPY-DMA-XOR** (lens finding M22, follow-up F-COPYXOR), size S, one branch. **Measure first, decide second: this parcel may correctly end with no emulation change.**

**First, tell me where this brief is wrong.** Every mechanism below is a hypothesis; your command output outranks it.

## The question

The VDP's VRAM **copy** DMA (`Vdp::run_copy`, `crates/oracle-core/src/vdp.rs`) reads and writes VRAM bytes at the plain address. The **fill** engine beside it writes at `address ^ 1`, pinned by a test ROM, on a forum statement by a hardware tester (Eke) that names copy DMA in the same sentence. Two separate hardware questions, and **they must be settled together, never one without the other:**

1. Does copy DMA's **write** land at `address ^ 1`?
2. Does copy DMA's **source read** come from `source ^ 1`?

Why together: if both are `^ 1`, a copy of an even-aligned, even-length span gives the SAME image as neither, and they differ only at odd start addresses, odd lengths, or odd autoincrements. If only the write is `^ 1`, every copied word comes out byte-swapped, which any commercial game using copy DMA would show on screen. Changing one half alone is the worst of the three models unless evidence picks it.

## Clean-room: binding, and the triage doc got it wrong

`vdp.rs`'s module doc: "no emulator source informs this code (clean-room, audit policy 3)". **Do not open, fetch, search or quote any emulator's source in any form**: BlastEm, Genesis Plus GX, Ares, jgenesis, Exodus, MAME, and this workspace's own `oracle-old` C++ core. That includes GitHub code search and blame views. `docs/2026-09-11-lens-triage.md:209` says "settle from BlastEm's `vdp.c`": ignore that line, and name it in your report as a brief error.
**Allowed evidence:** hardware documentation (Sega manuals, Nemesis's *VDP Internals*, Charles MacDonald's VDP notes, Plutiedev), forum PROSE from hardware testers (SpritesMind threads), test ROMs and their own expected-value tables, and what commercial ROMs in the committed corpus do and show. You may use WebFetch/WebSearch for those docs and threads, never for code.

## What to do

1. **Evidence, per half.** Collect what the allowed sources say about each half separately, quoted with a URL or file and line. Look for a test ROM that exercises copy DMA with an expected table (search the vendored suites under `vendor/` and `fixtures/`, and the ROM list in `docs/2026-07-25-testrom-conformance.md`). If none exists, say so, with the search that shows it.
2. **Measure what moves, before changing anything.** Instrument copy DMA across every committed ROM and fixture the test suite runs (a counter or a `last_dma` scan in a test or example; **no emulator MCP**): how many copy DMAs fire, and with what `(source, dest, len, autoinc)`. Classify each as aligned-even (both models agree) or odd (they differ). **If none fires, or all are aligned-even,** the both-halves model moves no frozen currency and the change is justified on the evidence from item 1. **If an odd one fires,** name the ROM, frame and currency it would move.
3. **Decide by this rule, and say which branch you took:**
   - A ROM expected table pins either half: implement what it pins, both halves, and add the table as a test.
   - No ROM, and the evidence names both halves or plainly implies the VDP's internal byte addressing (a VRAM byte access, read or write, lands at `^ 1`): implement **both halves**, **only if item 2 shows no frozen currency moves**. If currency would move, stop and report BLOCKED with the measurement. That is a ruling for me, not an edit for you.
   - The evidence supports only one half: do not implement it. Report, and leave `run_copy` unchanged.
4. **If you change `run_copy`:** unit tests that pin the new image at an odd start, an odd length and an odd autoincrement, each expected image **derived by hand from the rule** in the test comment (never read back from the implementation), plus the control that an aligned-even copy is unchanged from today's image. Update `run_copy`'s doc, the fill arm's "`run_copy` is deliberately NOT changed" sentence, and **every paraphrase of it**: vary the spelling (`run_copy`, "copy DMA", "VRAM copy", `F-COPYXOR`, `Q2`) across `crates/`, `docs/2026-07-25-testrom-conformance.md` and `docs/2026-08-03-a3-dma-fifo-design.md` (dated docs: add a dated resolution line, never rewrite history).
5. **Either way,** write the outcome into `F-COPYXOR`'s entry in `docs/2026-07-25-testrom-conformance.md` (a dated line: evidence, measurement, decision) and append one line to `docs/lens-findings.jsonl` for M22 (`"state":"fixed"` with `fixedAt` your fix commit if you changed code; otherwise an `open` line whose `detail` carries the measurement and what evidence would settle it). Copy the shape of the existing lines; never rewrite an old line.

## 0. Base check, branch, vendor, scratch

- `cd` to your worktree root (absolute path) before any git operation.
- Base check: `git merge-base --is-ancestor 9c5185f HEAD && echo OK` must print OK and `docs/2026-09-12-copy-dma-xor-brief.md` must exist. If the file is missing, run `git fetch origin && git merge --ff-only origin/main` once and re-check; still missing means BLOCKED, stop.
- `git switch -c parcel/copy-dma-xor`. Record your tip SHA as you go. **The controller (me) merges your branch into main and then deletes the branch and your worktree**; a missing branch afterwards is expected. Check `git merge-base --is-ancestor <tip> main` first, and confirm by content if that says no (a squash merge rewrites commits).
- `ln -s /home/volence/sonic_hacks/oracle/vendor vendor` (gitignored). `ls` is aliased to eza, so use `command ls`.
- Scratch: a run-unique dir you create, `/tmp/claude-1000/copy-dma-xor-$(date +%s)/`.
- **If `origin/main` moves while you work**, `tools/land.sh` refuses at G3. Then `git fetch origin && git merge origin/main` into your branch (never rebase) and re-run.

## Hard boundaries

- **Files in scope:** `crates/oracle-core/src/vdp.rs`, tests/examples you add, `docs/2026-07-25-testrom-conformance.md`, `docs/2026-08-03-a3-dma-fifo-design.md` (a dated line only), `docs/lens-findings.jsonl` (append only). **Not `crates/oracle-core/src/system.rs`**: another agent owns it right now. Anything else: say why in the report before touching it.
- **Never touch** `docs/lane-status.json`, `docs/lane-log.jsonl`, `docs/decisions.jsonl` or any `docs/OVERSEER*.md`, nor the vendored contract/schema.
- **No snapshot, `export_state` or `state_hash` layout change.** Only copy DMA's behaviour may change, and only under rule 3.
- **No emulator, ever.** Never call any `mcp__oracle__*` tool; never launch a window or BlastEm on any display. TAG anything that needs a live run.
- **BLOCKED is always available**: stop that item, record why, finish the rest. Never degrade a design to reach green.
- Do not merge, push, or open a PR.

## Every check you add follows these five rules

(a) **red-first, SHOWING THE MUTATION APPLIED** (quote the mutated line from disk or `git diff --stat` before the red run). The natural mutations: drop the `^ 1` from each half in turn, and show the odd-case test fails naming the half. (b) Restore from a **committed** baseline, never `git checkout --` over uncommitted work. (c) Actually red: applied-and-still-green means the runner isn't executing your patch (cargo fingerprints by mtime; watch for "Compiling"). (d) Wired into `cargo test --workspace`, expectations **derived** by hand from the rule, loud on unmeasurable. (e) If you tighten the method midway, re-establish earlier claims under it and say which. **For every assertion, ask: if this went green for a reason OTHER than the rule holding, what would that reason be?** (For example, an even-aligned case passes under all three models, so it proves nothing about either half.) Name the reason in the report and say how you ruled it out.

## Running and verifying

- **One cargo invocation at a time in your worktree.** Targeted runs (`cargo test -p oracle-core --lib vdp::`) while you work. A test that fails only under machine load is a defect with a narrow window, not a flake: report it by name.
- Final check: **`./tools/land.sh --no-push`** from your worktree root after your last commit, clean tree. **Detach it** per its header (run-unique wrapper in YOUR scratch dir writing `LAND-EXIT=$? AT $(date -Is)` to your log; `setsid nohup … &`), **then poll your own log in the FOREGROUND until LAND-EXIT appears**: `for i in $(seq 1 7); do grep -q '^LAND-EXIT=' "$LOG" && break; sleep 15; done`, repeated. **Do NOT end your turn while it runs, and never wait on a background-task notification.** Never foreground land.sh. Do not commit while it runs.
- Baseline, **release profile**, `tools/land.sh --no-push` at `efdd1a6`: **89/89 legs, 2867 passed / 0 failed / 3 ignored**. **Predict your leg and pass counts before the run** and report prediction vs result; a moved leg count is an explanation or a problem, so say which. Every frozen golden and currency constant must come out byte-identical, or you are in rule 3's BLOCKED branch.
- Report aggregate totals with the profile, never a tail; grep failures with `^test [^ ]+ \.\.\. FAILED`; capture exit codes outside pipes; match processes by exact name (`pgrep -x cargo`).
- Commit as each piece lands, **findings in the message** (a death must cost the run, never the work); messages from a file, read back with `git log -1 --format=%B`. Exact-path `git add` (never `-A`), `git show --stat` after each. No `Co-Authored-By`.

## Report shape

1. Where this brief was wrong. 2. Branch, tip SHA (from git), commits. 3. Evidence per half, quoted with sources; the test-ROM search and its result. 4. The corpus measurement: copy DMAs by ROM, aligned or odd. 5. Which branch of rule 3 you took, and why. 6. Tests by name, each mutation (line on disk → named failure → restore), the alternative green path ruled out. 7. Surfaces changed (code, docs, paraphrases, ledger line). 8. `land.sh --no-push`: verdict token, LAND-EXIT, predicted vs observed legs, passed/failed/ignored, profile. 9. Open items, TAGs, anything BLOCKED.
