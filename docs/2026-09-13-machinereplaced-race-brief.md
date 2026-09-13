# F-MACHINEREPLACED-EVENT-RACE: the intermittent "got 0" (the dispatched brief)

> **Dispatch artifact, 2026-09-13 (overseer, session 17).** The go is the hub's ruling (2a) applied under the owner's
> goodnight delegation, read at empyrean `737fbfc:docs/OVERSEER.md` lines 74-89 (`737fbfc` is an ancestor of their
> `origin/main`, a log/status tick commit, so it is a revision to read the words at, not the commit carrying them).
> Order is this repo's `docs/OVERSEER.md` "Order of work, 2026-09-13": this row, then M24 parcel 3, then M1.
> Premises measured at oracle `ff194d8`. Nothing under `crates/`, `tools/`, `examples/`, `Cargo.toml` or `Cargo.lock`
> changed between `453aa96` and `ff194d8`, so the last full baseline stands: **release profile,
> `tools/land.sh --no-push` at `453aa96`'s tip: 90/90 legs, 2907 passed / 0 failed / 3 ignored.**
> The text below the rule is the agent prompt, passed with `isolation: worktree`.

---

You are an implementation agent in the **oracle** repo (a from-scratch Rust Mega Drive emulator and its Aether debug bus, JSON-RPC over a Unix socket). One defect, one branch: an integration test that fails rarely and has now failed three times, twice in CI.

**First, tell me where this brief is wrong.** Everything below is a hypothesis; your command output outranks it. The last seven agents in this repo each corrected their brief on material points, measured. Say so first in your report.

## The defect, as observed

`crates/oracle-aether/tests/machine_replaced.rs`, test `hits_dropped_is_zero_and_PRESENT_on_a_window_state_load_when_nothing_was_recorded` (row 2), panics in `the_one_replacement` (~line 291): *"expected exactly one emulator/machineReplaced in the stream, got 0: []"*. Sightings: 2026-09-10 locally at load ~19.8 (then 31 green re-runs); CI run 34723609349 on `c86e1c3`; CI run 34745406490 on `ea1dcb8` (a docs-only commit). Always this row, never its siblings.

## The overseer's hypothesis (read from source, NOT measured; reproduce before you believe it)

An earlier session guessed "the event is emitted after its own reply". **I think that is wrong**, because `Engine::emit` (`engine.rs`, `fn emit`) broadcasts synchronously into each subscribed connection's `Outbound` during the gesture, which completes before the window thread acks, before the test sends its marker request. So the FIFO argument in `stream_to_marker`'s doc comment holds **for a subscribed connection**.

What I think it misses is **when the connection becomes subscribed**. `server.rs` (~line 762, `Action::Subscribe`) calls `subs.add(...)` when the reader thread processes the client's `initialized` **notification**, and a notification gets no reply (JSON-RPC 2.0). `tests/common/mod.rs` `Client::handshake` sends `initialized` as its last act and returns. Row 2 then goes straight to `w.gesture(Gesture::LoadState)`, which reaches the window thread over an in-process `mpsc` channel with no socket in between. If the gesture's `emit` runs before the reader thread has read `initialized`, the connection is not in `subs` yet and the event is never queued for it: "got 0", with **no `droppedEvents` count**, because nothing was dropped. Row 1 does three round trips (`record_some_hits`) between handshake and gesture, and a request read after `initialized` is necessarily processed after it on the same reader thread, which would explain why row 1 never fails.

What would make this wrong: row 2 failing with a round trip in place; the reader thread handling `initialized` somewhere other than I think; an `emit` path that is not synchronous. Check each.

## What to do

1. **Reproduce deterministically before fixing** (this repo's bar: a hazard fixed without reproducing it was half-misdiagnosed). Widen the suspected window with a temporary, on-disk-shown mutation (for example a short sleep in the reader thread just before `subs.add`, or anything better you find) and show row 2 going red **every time** with the exact production message, while row 1 stays green. Then show the same mutation plus your fix is green. If you cannot make it red this way, the hypothesis is wrong: stop, report what you measured, and try the next explanation rather than fixing on a guess.
2. **Decide where the fix belongs, and say why.** Candidates, not a ruling: (a) a barrier in row 2 / `attach()`; (b) a barrier inside the shared `Client::handshake(true)` (one cheap round trip after `initialized`), which fixes every subscribing test at once but changes what every such test sees on its stream first; (c) something server-side. **A change to what the wire does, or to what `protocol.md` promises about when events begin, is a contract change: STOP that path and report it BLOCKED with the reasoning; that is a ruling for me and the hub.** A test-harness fix plus a true comment is in scope.
3. **Enumerate every test with the same exposure, by what TOUCHES the race, not by the words of this one.** The shape: a subscribing client (`handshake(true)`, 15 files under `crates/oracle-aether/tests/` today; re-derive the list) followed by an event whose source is **not a request on that same connection** (a window gesture, a free-running host, another connection, a pumped `Host`) with no round trip in between. Vary the axis: also check tests that assert an **absence** of an event, since a missed registration turns those into vacuous passes, which is the more dangerous direction because nothing ever goes red. Report each file: exposed / not exposed, with the line that decides it. Fix the exposed ones the same way, or say why not.
4. **Repair the claims that restated the wrong premise**: `stream_to_marker`'s doc comment says soundness is "a property of the transport rather than of timing", which is true only once registered. Sweep for paraphrases of that claim (vary the spelling: "ahead of the reply", "single FIFO", "without a sleep", "would already be in this vector") across `crates/**`, and report phrasings × hits × verdict.
5. **Product exposure, report only:** does a real client connecting to the owner's window (`oracle-player`) have the same gap, where a state load landing between `initialized` and registration is silently not delivered? Say what a client can do about it today (a round trip after `initialized` is a barrier, if true) and whether `protocol.md` (read it at `empyrean` `origin/main` via `git -C ../empyrean show origin/main:contract/protocol.md`, never the sibling path) says anything about it. **No code change for this item**; it goes in the report for a ruling.
6. **Adjacent booking, do not chase:** `F-HANDSHAKE-LOAD-TIMEOUT` (`tests/handshake.rs::initialize_advertises_a_generated_method_list_that_is_the_dispatch_table`, a socket read timeout under load ~250) is booked as a different root cause. If your mechanism plainly explains it too, say so with evidence; otherwise leave it alone.

## 0. Base check, branch, vendor, scratch

- `cd` to your worktree root (absolute path) before any git operation.
- Base check: `git merge-base --is-ancestor ff194d8 HEAD && echo OK` must print OK and this brief (`docs/2026-09-13-machinereplaced-race-brief.md`) must exist. If it is missing, run `git fetch origin && git merge --ff-only origin/main` once and re-check; still missing means BLOCKED, stop.
- `git switch -c parcel/machinereplaced-subscribe-race`. Record your tip SHA as you go. **I (the controller) merge your branch into main and then delete the branch and your worktree**; a missing branch afterwards is expected. Check `git merge-base --is-ancestor <tip> main` first, and confirm by content if that says no (a squash merge rewrites commits).
- `ln -s /home/volence/sonic_hacks/oracle/vendor vendor` (gitignored), then check `vendor/` resolves (17 TestRoms entries). Without it 8 `save_state` rows fail and the 68000 SST sweep silently skips. `ls` is aliased to eza, so use `command ls`.
- Scratch: a run-unique dir you create, `/tmp/claude-1000/mr-race-$(date +%s)/`. Nothing of yours goes in any shared scratch path.

## Hard boundaries

- **Files in scope:** the test files your enumeration marks exposed, `crates/oracle-aether/tests/common/mod.rs` if you choose (b), and comment-only repairs from item 4. **No change under any `src/`** without stopping to report why first; a server-side fix is item 2's BLOCKED path.
- **Never touch** `docs/lane-status.json`, `docs/lane-log.jsonl`, `docs/decisions.jsonl`, `docs/lens-findings.jsonl` or any `docs/OVERSEER*.md`, nor the vendored contract/schema. Queue outcomes go in your report; I transcribe them.
- **No emulator, ever.** Never call any `mcp__oracle__*` tool; never launch a window on any display. TAG anything that needs a live run.
- **BLOCKED is always available**: stop that item, record why, finish the rest. Never degrade a design to reach green, and never "fix" a flake with a sleep or a retry: a barrier is an ordering guarantee; a sleep is a bet.
- Do not merge, push, or open a PR.

## Every gate you add follows these rules

(a) **Red-first with the mutation shown applied** before the red run (quote the mutated line from disk, or `git diff --stat` naming the file). (b) Restore from a **committed** baseline, never `git checkout --` over uncommitted work. (c) **Applied and still green is a finding about the instrument first**: cargo fingerprints by mtime, so watch for "Compiling"; fix the runner before claiming the gate. (d) Wired into a runner that executes it (name it), loud on unmeasurable, never a silent 0. (e) If you tighten a method midway, re-establish earlier claims under it and say which. **For every "it went red" or "it stayed green", ask what else would produce the same result** and say how you ruled it out. **Vary the mutation parameter**, do not repeat one mutation. For an absence-asserting row, show it goes red when the event it says is absent is really emitted, or it proves nothing.

After the fix, also run the fixed row(s) in a loop (at least 200 runs of the test binary, `--test machine_replaced`, with `cat /proc/loadavg` before and after) and report the count. That is a sanity check, not the proof; the deterministic widened-window run is the proof.

## Running and verifying

- **One cargo invocation at a time in your worktree**; no other agent is running cargo in this repo right now. Every timing figure ships with `cat /proc/loadavg` and wall-clock uptime; peers share this box.
- Any OTHER failure that only appears under load is a defect with a narrow window, not a flake: report it by name.
- Final check, after your last commit, clean tree: **`./tools/land.sh --no-push`** from your worktree root. **Detach it** per its header (a run-unique wrapper in YOUR scratch dir writing `LAND-EXIT=$? AT $(date -Is)` to your log; `setsid nohup … &`), **then poll your own log in the FOREGROUND until LAND-EXIT appears**: `for i in $(seq 1 7); do grep -q '^LAND-EXIT=' "$LOG" && break; sleep 15; done`, repeated. **Do NOT end your turn while it runs, and never wait on a background-task notification.** Never foreground land.sh. Do not commit while it runs. The script is `tools/land.sh`; read the `LAND-EXIT` token in the log, never a notification's exit code.
- Baseline (release profile, `tools/land.sh --no-push`): **90/90 legs, 2907 passed / 0 failed / 3 ignored.** Predict your legs and passed/ignored counts before the run and report prediction vs result, with the profile named on every count.
- Report aggregate totals, never a tail; grep failures with `^test [^ ]+ \.\.\. FAILED`; capture exit codes outside pipes; match processes by exact name (`pgrep -x cargo`).
- Commit as each piece lands, **findings in the message** (a death must cost the run, never the work); write messages to a file and commit with `-F <file>` (never `-F -`, never backquotes inside `$(…)`), read back with `git log -1 --format=%B`. Exact-path `git add` (never `-A`), `git show --stat` after each. No `Co-Authored-By`.

## Report shape

1. Where this brief was wrong. 2. Branch, tip SHA (from git), commits, and `git diff --stat ff194d8...<tip>`. 3. The mechanism as measured: the deterministic repro (mutation on disk, red output quoted, row 1's control), and what else could have produced it and how you ruled it out. 4. The fix, where it lives and why there; anything BLOCKED as a contract question. 5. The exposure enumeration (file × exposed? × deciding line × fixed?), including absence-asserting rows. 6. The paraphrase sweep (phrasings × hits × verdict). 7. Product exposure and what `protocol.md` says (item 5). 8. The 200-run loop count with load. 9. `land.sh --no-push`: verdict token, LAND-EXIT, predicted vs observed legs, passed/failed/ignored, profile. 10. Open items and TAGs.
