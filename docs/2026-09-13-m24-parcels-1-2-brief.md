# M24 parcels 1 and 2: the F-Z80 caveat, then the Z80 timing probes (the dispatched brief)

> **Dispatch artifact, 2026-09-13 (overseer, session 16).** The go is the hub's ruling (2) applied under the owner's
> goodnight delegation (read at empyrean `f1220de:docs/OVERSEER.md` lines 74-89; `f1220de` is an ancestor of their
> `origin/main`, a log-only tick commit, so it is a revision to read the words at). The order is this repo's
> `docs/OVERSEER.md` "Order of work, 2026-09-13": design §7 parcels 1 then 2, neither needing a ruling. This closes
> **M24-NEEDS-A-CURRENCY**.
> **One agent, both parcels, two commit groups, one landing** — a change from the "one agent after the other" the
> previous session wrote, with cause: the parcels touch disjoint files, neither moves emulation bytes, a frozen
> currency or the wire, so the design's "land it first and alone" (§5, which keeps the caveat apart from the byte-moving
> fixes and the hook) still holds commit-by-commit; one landing saves a full-suite run on a shared machine.
> Premises measured at oracle `b122d44`. Nothing under `crates/`, `tools/`, `examples/`, `Cargo.toml` or `Cargo.lock`
> changed between `9a09adc` and `b122d44` (`git diff --stat` empty), so the last full baseline stands: **release profile,
> `tools/land.sh --no-push` at the M24 design tip: 89/89 legs, 2898 passed / 0 failed / 3 ignored.**
> The text below the rule is the agent prompt, passed with `isolation: worktree`.

---

You are an implementation agent in the **oracle** repo (a from-scratch Rust Mega Drive emulator and its Aether debug bus). Two small parcels, one branch, in order: **parcel 1** (a watchpoint caveat, XS) then **parcel 2** (Z80 timing probe tests, S). Your authority is the design doc `docs/2026-09-13-z80-timing-currency-design.md`: read its plain summary, §0, §1, §2 (all of it), §5, §6 and §7 before writing anything.

**First, tell me where this brief is wrong.** Every claim below is a hypothesis; your command output outranks it. The last six agents in this repo each corrected their brief on material points, measured. Do that here too, and say so first in your report.

## Parcel 1: the cheap half of F-Z80-ACCESSES-UNWATCHED (design §5)

Moves caveat strings and one doc header only. **No emulation bytes, no frozen currency, no Aether wire.**

1. **Make `crates/oracle-core/src/watchpoints.rs`'s module header true.** It says the real 68000/Z80 bus adapters deliver every access through `on_event_at`; the Z80 adapter delivers only its FM/PSG register writes. §5 gives the replacement sentence. **Then sweep for what RESTATES the false claim, not only for its words** (this repo's standing lesson: a claim repaired at its canonical site leaves its paraphrases standing, and they are where the next parcel falsifies it). Vary the spelling and the axis: "every access", "all accesses", "both CPUs", "Z80 … deliver", doc comments on `BusEventSink`/`Watchpoints`/`Z80Bus`, the player's watch-panel help text, README/docs prose. Report the enumeration (each phrasing searched, each hit, fixed or cleared with why).
2. **Caveat 1** from `Watchpoints::caveats()` for any bus-space watch overlapping a Z80-reachable 68000 range, and **caveat 2** for a write-matching bus-space watch overlapping `$004000-$004003` or `$007F11` (§0.8's collision). Wording in §5; you may tighten it, keep the substance. **Derive the Z80-reachable ranges from `crates/oracle-core/src/z80/bus.rs` (`read_window` and the bank register's reach), never copy §5's list**; if the code says the reach differs from §5, the code wins and you say so.
3. **Readers:** `crates/oracle-player/src/stopping.rs` (watch panel), `examples/diag_soundqueue.rs`, and `tests/watchpoints.rs::hits_carry_a_monotonic_master_clock_consistent_with_the_frame`, whose "no caveats on a plain `$FF0000` watch" assertion is re-derived with a `cause:` comment naming this caveat. Enumerate every other reader of `caveats()` by grep over the whole workspace (callers, not just the definition). **Check the design's claim that `oracle-aether` does not serialise `caveats()`.** If any caveat reaches the Aether wire (a method's reply), STOP that item: a wire-visible change is a contract change request, which is a ruling for me. Report it BLOCKED with the call path.
4. Tests: each caveat has a row that fires on an overlapping subject and stays silent on a non-overlapping one, with the boundary addresses derived from the ranges in (2) (just inside, just outside). Red-first per the rules below.
5. Commit parcel 1 on its own (one or more commits), findings in the message, before starting parcel 2.

## Parcel 2: the Z80 timing probes, C1-C4 (design §1 row (e), §2.1, §7 row 2)

**A new test file and nothing else** (suggested `crates/oracle-core/tests/z80_timing_probes.rs`; your call if a different home is the honest one, say why). No change to any file under `src/`. If a probe cannot be written through the existing public API, **STOP that probe and report BLOCKED** naming the missing capability; never widen `src/` to make a test possible.

Each probe is a tiny Z80 program on the `testrom::build` machine (or the smallest harness that runs the Z80 released), which **stores what it observed into Z80 RAM**; the test reads it back. The spike commits `0b1dd3b` and `aa04b7b` (`crates/oracle-replay/tests/spike_m24.rs`, `crates/oracle-core/src/spike_m24.rs`) hold the measured prototypes. **Mine them for program shape, but they leaned on an in-core thread-local probe that does not exist on `main`**, so re-derive how each observable reaches Z80 RAM.

- **C1, `EI` delay.** A pending request plus `EI; INC A; …`; the handler stores `A`. Today `A = 0`; UM0080 p.18 says the instruction after `EI` runs first, so documented `A = 1`.
- **C2, `/INT` width.** The Z80 enables late in the frame (the spike: line 226, two lines after the assert) and counts `INC HL`/`JR` passes; the handler stores the VDP V counter and `HL`. Today V = `$E2`, HL = 0; R6 (`docs/2026-07-16-vdp-recon.md`, one line ≈ 228 Z80 clocks) documents V = `$E0`, HL ≈ 3290-3293 (§2.1's derivation; re-derive it from the constants, do not copy 3291).
- **C3, grant timing.** Drive a grant schedule around gated-on spans; a 34-T-state loop counts passes. Expected count = gated-on time / 510 mclk **derived from the schedule the test itself chose**, never read back from the emulator's own accounting. ⚑ An assertion against the quantity under test is circular and stays green when that quantity is mutated (this repo's `CartBanks::IDENTITY` lesson). C3 is already right today and guards M21.
- **C4, level re-trigger.** A short handler that re-enables inside the one-line window and counts its entries. Today acceptance consumes the request, so derive today's count from the code path (`Z80::accept_interrupt`, `EventKind::VInt` in `system.rs`) and state it; document what R6's level model predicts, and **mark that prediction medium-confidence**, as §6.2 does (R6's masking corollary is the part parcel 4 needs a ruling on).

**Pinned at TODAY's values** (the ruled design, so the M24 fixes in parcels 3 and 4 must move them by a predicted amount). Beside each constant: the documented value, its source (manual page or recon section), and a line saying which parcel is expected to move it (C1 and C2's HL: parcel 3; C2's V and HL, and C4: parcel 4; C3: nothing in M24). A probe pinned at a value the documentation says is wrong must say so in plain words in its own comment, so no reader takes the pin for the truth.

**Selectivity is the point of these tests, so prove it.** Using mutations applied temporarily (not committed; env-gating like the spike's `M24_MUT` is fine in a scratch commit you drop), show for each probe that it goes **red under its own behaviour** and **stays green under the others'**: at minimum the spike's `ei`, `int1` and `m21` mutations, and one timing-neutral control. Report a probe × mutation table like §2.1's, each cell with the mutation's on-disk proof and, where a mutation cannot fire in that probe (for example `m21` in a probe that drives no grants), say "unmeasured" and not "still". Then restore from the committed baseline.

## 0. Base check, branch, vendor, scratch

- `cd` to your worktree root (absolute path) before any git operation.
- Base check: `git merge-base --is-ancestor b122d44 HEAD && echo OK` must print OK and `docs/2026-09-13-m24-parcels-1-2-brief.md` must exist. If it is missing, run `git fetch origin && git merge --ff-only origin/main` once and re-check; still missing means BLOCKED, stop.
- `git switch -c parcel/m24-caveat-and-probes`. Record your tip SHA as you go. **The controller (me) merges your branch into main and then deletes the branch and your worktree**; a missing branch afterwards is expected. Check `git merge-base --is-ancestor <tip> main` first, and confirm by content if that says no (a squash merge rewrites commits).
- `ln -s /home/volence/sonic_hacks/oracle/vendor vendor` (gitignored), then check `vendor/` resolves (17 TestRoms entries). Without it 8 `save_state` rows fail and the 68000 SST sweep silently skips. `ls` is aliased to eza, so use `command ls`.
- Scratch: a run-unique dir you create, `/tmp/claude-1000/m24-probes-$(date +%s)/`. Nothing of yours goes in any shared scratch path.

## Hard boundaries

- **Files in scope:** parcel 1: `watchpoints.rs` (header, `caveats()`), the caveat readers named above, the one re-derived test and new caveat rows, plus any paraphrase site your sweep finds (say which). Parcel 2: the one new test file. Anything else: say why in the report before touching it.
- **Never touch** `docs/lane-status.json`, `docs/lane-log.jsonl`, `docs/decisions.jsonl`, `docs/lens-findings.jsonl` or any `docs/OVERSEER*.md`, nor the vendored contract/schema. Queue outcomes go in your report; I transcribe them.
- **No M24 fix** (no `EI` shadow, no `/INT` change) and **no F-Z80 full half** (no Z80 access hook). Those are parcels 3-5.
- **No emulator, ever.** Never call any `mcp__oracle__*` tool; never launch a window on any display. TAG anything that needs a live run or a listen.
- **BLOCKED is always available**: stop that item, record why, finish the rest. Never degrade a design to reach green.
- Do not merge, push, or open a PR.

## Every gate you add follows these rules

(a) **Red-first with the mutation shown applied** before the red run (quote the mutated line from disk, or `git diff --stat` naming the file). (b) Restore from a **committed** baseline, never `git checkout --` over uncommitted work. (c) **Applied and still green is a finding about the instrument first**: cargo fingerprints by mtime, so watch for "Compiling"; fix the runner before claiming the gate. (d) Wired into a runner that executes it (name it), expectations **derived** from constants and documentation (`MCLK_PER_Z80_CYCLE`, `MCLK_PER_LINE`, instruction T-states from UM0080), never read back from the implementation; loud on unmeasurable, never a silent 0. (e) If you tighten a method midway, re-establish earlier claims under it and say which. **For every "it moved" or "it did not move", ask what else would produce the same result** and say how you ruled it out. **Vary the mutation parameter**, do not repeat one mutation: a guard's hole is usually one mutation away from the one its author tried.

## Running and verifying

- **One cargo invocation at a time in your worktree**; no other agent is running cargo in this repo right now. Targeted runs while you work. Every timing figure ships with `cat /proc/loadavg` and wall-clock uptime; peers share this box.
- A known intermittent failure exists and is not yours: `crates/oracle-aether/tests/machine_replaced.rs:291`, *"expected exactly one emulator/machineReplaced in the stream, got 0"* (F-MACHINEREPLACED-EVENT-RACE). If it appears, report it by name and do not chase it. Any OTHER failure that only appears under load is a defect with a narrow window, not a flake: report it by name.
- Final check, after your last commit, clean tree: **`./tools/land.sh --no-push`** from your worktree root. **Detach it** per its header (a run-unique wrapper in YOUR scratch dir writing `LAND-EXIT=$? AT $(date -Is)` to your log; `setsid nohup … &`), **then poll your own log in the FOREGROUND until LAND-EXIT appears**: `for i in $(seq 1 7); do grep -q '^LAND-EXIT=' "$LOG" && break; sleep 15; done`, repeated. **Do NOT end your turn while it runs, and never wait on a background-task notification.** Never foreground land.sh. Do not commit while it runs. The script is `tools/land.sh`; read the `LAND-EXIT` token in the log, never a notification's exit code.
- Baseline (release profile, `tools/land.sh --no-push`): **89/89 legs, 2898 passed / 0 failed / 3 ignored.** Predict your legs and passed/ignored counts before the run (a new integration test file is a new leg) and report prediction vs result, with the profile named on every count.
- Report aggregate totals, never a tail; grep failures with `^test [^ ]+ \.\.\. FAILED`; capture exit codes outside pipes; match processes by exact name (`pgrep -x cargo`).
- Commit as each piece lands, **findings in the message** (a death must cost the run, never the work); write messages to a file and commit with `-F <file>` (never `-F -`, never backquotes inside `$(…)`), read back with `git log -1 --format=%B`. Exact-path `git add` (never `-A`), `git show --stat` after each. No `Co-Authored-By`.

## Report shape

1. Where this brief was wrong. 2. Branch, tip SHA (from git), commits grouped by parcel, and `git diff --stat b122d44...<tip>`. 3. Parcel 1: the header sentence and both caveat texts as landed; the Z80-reachable ranges as derived from `bus.rs` (vs §5); every `caveats()` reader and what it does with the new text; whether anything reaches the wire; the paraphrase sweep (phrasings × hits × verdict). 4. Parcel 2: each probe's program in one line, its pinned value, its documented value with source, and which parcel should move it; the probe × mutation table with on-disk proofs. 5. Red-first proofs for every new row. 6. `land.sh --no-push`: verdict token, LAND-EXIT, predicted vs observed legs, passed/failed/ignored, profile. 7. Open items, TAGs, anything BLOCKED.
