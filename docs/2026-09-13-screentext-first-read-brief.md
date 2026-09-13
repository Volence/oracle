# F-PLAYER-SCREENTEXT-FIRST-READ: the window test that read the bar before the first draw (the dispatched brief)

> **Dispatch artifact, 2026-09-13 (overseer, session 18).** The go is the hub's, at 15:4xZ by message: ruling (2a)
> applied, not a new ruling. Read firsthand at empyrean `737fbfc:docs/OVERSEER.md` lines 74-89 (`737fbfc` is an
> ancestor of their `origin/main`; a log/status tick commit, so a revision to read the words at, not the commit
> carrying them). The hub accepted this lane's sequencing at empyrean `a127e0d` (ancestor, verified; its newest
> `docs/OVERSEER-LOG.md` entry). Order is this repo's `docs/OVERSEER.md` "Order of work", lines 98-100: this row,
> then M24 parcel 3, then M1.
> Premises measured at oracle `d7fc09f`. `git diff --stat 7ccd42d d7fc09f` names only `docs/OVERSEER.md` and
> `docs/lane-log.jsonl`, so the last full baseline stands: **release profile, `tools/land.sh --no-push` at
> `7ccd42d`'s tree: 90/90 legs, 2907 passed / 0 failed / 3 ignored.**
> The text below the rule is the agent prompt, passed with `isolation: worktree`.

---

You are an implementation agent in the **oracle** repo (a from-scratch Rust Mega Drive emulator, its Aether debug bus (JSON-RPC over a Unix socket), and `oracle-player`, the egui debug window). One defect, one branch: a window test that failed once in CI and has not been reproduced.

**First, tell me where this brief is wrong.** Everything below is a hypothesis; your command output outranks it. The last eight agents in this repo each corrected their brief on material points, measured. Say so first in your report.

## The defect, as observed

`crates/oracle-player/src/main.rs`, `mod loop_tests`, test `a_client_reads_this_windows_top_bar_and_it_follows_the_run_state` (~line 2298). One sighting: CI run 34752339602 on `453aa96`, job "Build, test, clippy, fmt". The client thread panicked at `main.rs:2369` (the `call` closure's error assert):

```
emulator/screen_text failed: {"code":-32005,"data":{"droppedEvents":0,"frame":1,"mclk":896042,"reason":"noDisplay","running":true},"message":"this server has no window; screen text exists only in a hosted player"}
```

then the main thread at `main.rs:2432` (`client.join()`). The oracle-player leg read `433 passed; 1 failed`. The next commit, `ff194d8`, has the same code and passed. The client declines events and never sends `initialized`, so this is **not** the subscribe-registration race fixed at `7ccd42d` (that fix lives in `crates/oracle-aether/tests/common/mod.rs`, which this test does not use).

## The overseer's hypothesis (read from source, NOT measured; reproduce before you believe it)

The test's own premise (1), in-process before the client is spawned, asserts that `emulator/screen_text` refuses `noDisplay` because nothing has been drawn. Its first client read (`running`, ~line 2386) then assumes the window has presented at least once. **I think nothing guarantees that.** The ordering inside `Loop::iterate` (~line 758) as I read it: adopt the run state (~761), then `bus::drain` (~846, one bounded non-blocking `Host::pump`, answering queued client requests), then the machine keys, the frame run and upload, then `build_ui`, and **only then** `self.bus.set_screen_text(...)` (~line 1002). `Engine::screen_text` (`crates/oracle-aether/src/engine.rs`, ~line 3916) refuses `noDisplay` while its `screen_text` field is `None`, which it is until the first `set_screen_text`.

So if the client connects, and its `initialize`, `emulator/status` and first `emulator/screen_text` are answered by a drain that runs **before the first iteration's publish**, the read is refused, with `frame: 1` if the frame advanced first. That needs either several of the client's sequential round trips served in one `Host::pump` (does it keep looping while new requests arrive mid-pump?), or a drain ordering different from mine. **I have not established which, and `frame: 1` has to be explained by whatever you find**: my reading of the order would make the first drain see frame 0 or 1 depending on where the frame actually runs relative to the drain. Check it rather than trusting my line numbers.

What would make this wrong: the drain sitting after the publish; `Host::pump` answering at most one request per iteration (then three round trips need three iterations, and iteration 1's publish comes first); `initialize` or `status` being answered off the window thread entirely; a publish path I missed.

## What to do

1. **Reproduce deterministically before fixing** (this repo's bar: a hazard fixed without reproducing it was half-misdiagnosed). Widen the suspected window with a temporary mutation shown on disk (for example, make the client's first read race the first iteration by having the main thread block before its first `iterate` until the client has sent its first `screen_text`, or delay the first `set_screen_text`, or anything better you find). Show the test going red **every time** with the exact production refusal (`reason: noDisplay`, `frame` as observed), and explain the `frame` value you get. Then show the same mutation plus your fix is green. If you cannot make it red, the hypothesis is wrong: stop, report what you measured, and try the next explanation rather than fixing on a guess.
2. **Decide where the fix belongs, and say why.** Candidates, not a ruling:
   - (a) **test-side:** the client waits on an *observable* that the window has presented before its first `screen_text` read. `emulator/status` already reports `"display": self.screen_text.is_some()` (engine.rs ~3809), so polling `status` with a failing deadline is an ordering guarantee. So is the main thread running one iteration before it spawns the client, as long as premise (1)'s in-process `noDisplay` probe stays before that iteration.
   - (b) **product-side, window only:** make the first publish precede the first drain that can answer a client, so a client of a real window never sees `noDisplay` from a window that exists.
   - (c) **anything that changes what the wire says**, for example a new reason distinguishing "not drawn yet" from "no window": that is a contract change. **STOP that path and report it BLOCKED with the reasoning.** It is a ruling for me and the hub.

   Whichever you pick, **the test must keep its anti-vacuity design**: premise (1) still asserts `noDisplay` in-process before anything is drawn, and the two reads still start out of step (running, then paused). A barrier that also lets the first read witness nothing is a regression.
3. **Enumerate every test with the same exposure, by what TOUCHES the race, not by the words of this one.** The shape: a client reading a value the window **publishes after the drain** (`set_screen_text`, `set_pacing` (host.rs ~418 says it is the same seam, one instrument over), and any other `Host::set_*` fed from `Loop::iterate` or `oracle-frontend`'s loop), with no guarantee that a publish has happened before the read. Vary the axis: check `crates/oracle-player`, `crates/oracle-frontend` and `crates/oracle-aether/tests`, and check reads that **expect** a refusal as well as reads that expect a value. Report each site as exposed or not exposed, with the line that decides it, and whether you fixed it.
4. **Repair claims that restate the wrong premise.** Comments near the publish (`main.rs` ~972-986, `bus.rs` ~737-746, `host.rs` ~393-411) say a mid-session client never reads `noDisplay` from a live window. Check whether any of them claim, or imply, that a **start-of-session** client cannot either. If one does, repair it to what you measured. Sweep for paraphrases of the claim with varied spellings (for example "until the next present", "never on one mid-composition", "on the glass", "first present", "no window") across `crates/**`, and report phrasings × hits × verdict.
5. **Product exposure, report only:** does a real client that attaches to the owner's window during its first frames get `"this server has no window"` from a window that exists? Estimate how wide that gap is, and say what `contract/protocol.md` §11.29 promises about `noDisplay`. Read it at empyrean `origin/main` via `git -C ../empyrean show origin/main:contract/protocol.md`, never the sibling path. **No wire change for this item**; it goes in the report for a ruling.

## 0. Base check, branch, vendor, scratch

- `cd` to your worktree root (absolute path) before any git operation.
- Base check: `git merge-base --is-ancestor d7fc09f HEAD && echo OK` must print OK, and this brief (`docs/2026-09-13-screentext-first-read-brief.md`) must exist. If it is missing, run `git fetch origin && git merge --ff-only origin/main` once and re-check. Still missing means BLOCKED: stop.
- `git switch -c parcel/screentext-first-read`. Record your tip SHA as you go. **I (the controller) merge your branch into main and then delete the branch and your worktree**; a missing branch afterwards is expected. Check `git merge-base --is-ancestor <tip> main` first, and confirm by content if that says no (a squash merge rewrites commits).
- `ln -s /home/volence/sonic_hacks/oracle/vendor vendor` (gitignored), then check that `vendor/` resolves (17 TestRoms entries). Without it, 8 `save_state` rows fail and the 68000 SST sweep silently skips. `ls` is aliased to eza, so use `command ls`.
- Scratch: a run-unique directory you create, `/tmp/claude-1000/st-first-read-$(date +%s)/`. Nothing of yours goes in any shared scratch path.

## Hard boundaries

- **Files in scope:** the test(s) your enumeration marks exposed; `crates/oracle-player/src/main.rs` (`Loop::iterate`) if you choose (b); comment-only repairs from item 4. **No change under `crates/oracle-aether/src/` or `crates/oracle-core/`** without first stopping to report why. A change to what the bus answers is item 2(c)'s BLOCKED path.
- **Never touch** `docs/lane-status.json`, `docs/lane-log.jsonl`, `docs/decisions.jsonl`, `docs/lens-findings.jsonl`, any `docs/OVERSEER*.md`, or the vendored contract/schema. Queue outcomes go in your report; I transcribe them.
- **No emulator, ever.** Never call any `mcp__oracle__*` tool, and never launch a window on any display. The test drives `Loop::iterate` against a headless `egui::Context`, which is fine. TAG anything that needs a live window.
- **BLOCKED is always available**: stop that item, record why, finish the rest. Never degrade a design to reach green, and never "fix" a flake with a sleep or a retry. A barrier on an observable is an ordering guarantee; a sleep is a bet.
- Do not merge, push, or open a PR.

## Every gate you add follows these rules

(a) **Red-first, with the mutation shown applied** before the red run (quote the mutated line from disk, or `git diff --stat` naming the file). (b) Restore from a **committed** baseline, never `git checkout --` over uncommitted work. (c) **Applied and still green is a finding about the instrument first**: cargo fingerprints by mtime, so watch for "Compiling", and fix the runner before claiming the gate. (d) Wired into a runner that executes it (name it), loud on unmeasurable, never a silent 0. (e) If you tighten a method midway, re-establish earlier claims under it and say which. **For every "it went red" or "it stayed green", ask what else would produce the same result** and say how you ruled it out. **Vary the mutation parameter**; do not repeat one mutation. The fixed test must still go red under each of its own four documented alternative green paths (doc comment ~2284-2296) that your change touches; show at least the one nearest your change.

After the fix, also run the fixed test in a loop: build the test binary once, then run it at least 500 times filtered to the one test, with `cat /proc/loadavg` before and after, and report the count. That is a sanity check, not the proof; the deterministic widened-window run is the proof.

## Running and verifying

- **One cargo invocation at a time in your worktree**; no other agent is running cargo in this repo right now. Every timing figure ships with `cat /proc/loadavg` and wall-clock uptime, because peers share this box.
- Any OTHER failure that only appears under load is a defect with a narrow window, not a flake: report it by name.
- Final check, after your last commit, on a clean tree: **`./tools/land.sh --no-push`** from your worktree root. **Detach it** per its header: a run-unique wrapper in YOUR scratch dir writing `LAND-EXIT=$? AT $(date -Is)` to your log, launched with `setsid nohup … &`. **Then poll your own log in the FOREGROUND until LAND-EXIT appears**: `for i in $(seq 1 7); do grep -q '^LAND-EXIT=' "$LOG" && break; sleep 15; done`, repeated. **Do NOT end your turn while it runs, and never wait on a background-task notification.** Never run land.sh in the foreground, and do not commit while it runs. The script is `tools/land.sh`. Read the `LAND-EXIT` token in the log, never a notification's exit code.
- Baseline (release profile, `tools/land.sh --no-push`): **90/90 legs, 2907 passed / 0 failed / 3 ignored.** Predict your legs and your passed/ignored counts before the run, report prediction vs result, and name the profile on every count.
- Report aggregate totals, never a tail. Grep failures with `^test [^ ]+ \.\.\. FAILED`, capture exit codes outside pipes, and match processes by exact name (`pgrep -x cargo`).
- Commit as each piece lands, **with the findings in the message** (a death must cost the run, never the work). Write messages to a file and commit with `-F <file>` (never `-F -`, never backquotes inside `$(…)`), then read back with `git log -1 --format=%B`. Use exact-path `git add` (never `-A`), and run `git show --stat` after each commit. No `Co-Authored-By`.

## Report shape

1. Where this brief was wrong.
2. Branch, tip SHA (from git), commits, and `git diff --stat d7fc09f...<tip>`.
3. The mechanism as measured: the deterministic repro (mutation on disk, red output quoted, the `frame` value explained), what else could have produced it, and how you ruled that out.
4. The fix, where it lives and why there; anything BLOCKED as a contract question.
5. The exposure enumeration (site × exposed? × deciding line × fixed?), including reads that expect a refusal.
6. The paraphrase sweep (phrasings × hits × verdict).
7. Product exposure and what §11.29 says (item 5).
8. The 500-run loop count, with load.
9. `land.sh --no-push`: verdict token, LAND-EXIT, predicted vs observed legs, passed/failed/ignored, profile.
10. Open items and TAGs.
