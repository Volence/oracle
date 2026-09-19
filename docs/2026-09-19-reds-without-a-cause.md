# F-REDS-WITHOUT-A-CAUSE — dispositions for the eight unchased CI reds

**Date:** 2026-09-19 · **Branch:** `parcel/reds-without-a-cause` · **Base:** `52178b8`
**Kind:** investigation. Nothing was fixed here. Every disposition below is derived from the tree
or from a run in this document, never from a prior note's own verdict (RULE 1).

---

## The headline number

**UNDETERMINED: 0 of 8.**

That is not a claim that the reds were clean. It is the opposite: **all eight were real defects, all
eight are already fixed, and the eight runs collapse into FOUR distinct defects.** Two of the four
were diagnosed at the time; two were the events already chased tonight.

The reason the count is zero is uncomfortable and is the actual finding of this row: **the premise
that the evidence was gone was false.** Every one of the eight runs still has a complete log naming a
failing test and an assertion string. Nothing needed to be reproduced blind, and no SHA needed a cold
suite build to be dispositioned (one was built anyway, as a control — see Event C).

---

## The extractor, and its positive control

The brief's survey was produced by an extractor that had already been wrong once on this task. So the
extractor used here was controlled before it was trusted.

**Positive control**, as specified — run `34752339602` is known to contain `noDisplay` exactly once:

```
$ grep -c 'noDisplay' 34752339602.log
1
$ grep -n 'noDisplay' 34752339602.log
4939:...Test 2026-09-13T11:16:30Z emulator/screen_text failed: {"code":-32005,"data":{"droppedEvents":0,
      "frame":1,"mclk":896042,"reason":"noDisplay","running":true},"message":"this server has no window;
      screen text exists only in a hosted player"}
```

Exactly one hit, and the line carries the `{"frame":1,"mclk":896042,"reason":"noDisplay"}` payload the
brief quotes. **Negative control:** the same grep against `35051831548.log` returns `0`. The extractor
both fires and declines to fire.

Retrieval was `gh run view <id> --log`, exit code checked per run, stderr captured separately.
**All ten runs returned exit 0 with a non-empty log.**

---

## The survey table was wrong in three ways

| the brief said | what re-derivation shows |
|---|---|
| "the other **eight**" — then listed **seven** rows | The table is short by one. The missing run is `462e9cf` / `35418036061`. |
| `82e812a`, `85c1599`, `adcf239` — "**logs gone**", 0 lines | All three logs download in full: **1683 / 1675 / 1692 lines**, each naming a failing test and an assertion. This is the *second* time the extraction reported absent data that was present. |
| "died ~3 legs in" / "died ~24 legs in", read off `test result:` counts | The count is not a progress meter. A run's log concatenates **all three jobs**; the 09-16 pair never reached the Test step at all (they died in **Clippy**), so their "3 legs" are other jobs' legs entirely. |

The `log lines` column was right. Every reading drawn from it was wrong.

---

## The eight, dispositioned

Four events, eight runs. Priced once per event.

| # | SHA | run | date | disposition | event |
|---|---|---|---|---|---|
| 1 | `462e9cf` | `35418036061` | 09-19 | **REAL → FIXED-SINCE `1235148`** | D — panel masked-digit |
| 2 | `549f020` | `35051763771` | 09-16 | **REAL → FIXED-SINCE `0647d6f`** | A — floor clippy |
| 3 | `c03ebea` | `35051831548` | 09-16 | **REAL → FIXED-SINCE `0647d6f`** | A — same cause as #2 |
| 4 | `c86e1c3` | `34723609349` | 09-12 | **REAL → FIXED-SINCE `c6ed909`** | B — machineReplaced race |
| 5 | `ea1dcb8` | `34745406490` | 09-13 | **REAL → FIXED-SINCE `c6ed909`** | B — same cause as #4 |
| 6 | `adcf239` | `34649321849` | 09-11 | **REAL → FIXED-SINCE `d0844ce`** | C — inode reuse |
| 7 | `85c1599` | `34659398853` | 09-11 | **REAL → FIXED-SINCE `d0844ce`** | C — same cause as #6 |
| 8 | `82e812a` | `34660816320` | 09-12 | **REAL → FIXED-SINCE `d0844ce`** | C — same cause as #6 |

**INFRA: 0. UNDETERMINED: 0.** No run died for a reason outside our code; every one of the eight
failed the `Build, test, clippy, fmt` job on our own gate, with `Determinism gate` and
`Replay playthroughs (release)` **green in all eight**. That asymmetry is itself the evidence against
INFRA: a runner, network or OOM event would not spare two sibling jobs eight times out of eight.

---

### Event A — the floor-clippy lint (runs 2, 3)

**What failed.** Not a test. The **Clippy (deny warnings)** step, deterministically, at the same line
in both runs:

```
error: this boolean expression can be simplified
  --> crates/oracle-core/src/vdp.rs:986:16
986 |    if !(self.vblank(line_start) || !self.display_enabled()) {
    |       help: try: `!self.vblank(line_start) && self.display_enabled()`
    = note: `-D clippy::nonminimal-bool` implied by `-D warnings`
error: could not compile `oracle-core` (lib) due to 1 previous error
```

Byte-identical in `35051763771` (line 906) and `35051831548` (line 1404). `549f020` is the P1 FILL-RUN
merge; `c03ebea` is the lane-log commit on top of it, carrying the same source line. **One event, two
runs.** Job durations were 57 s and 47 s — the shortness is the clippy step failing, not a suite dying.

**FIXED-SINCE `0647d6f`**, the very next commit on `main` after `c03ebea`. The causal link is exact,
not circumstantial: `0647d6f` rewrites *that* expression at *that* site into *clippy's own suggested
form*, and `HEAD` carries it —

```
$ git grep -n 'vblank(line_start)' HEAD -- crates/oracle-core/src/vdp.rs
HEAD:crates/oracle-core/src/vdp.rs:990:    if !self.vblank(line_start) && self.display_enabled() {
```

**Why this was never a flake, and why it got through.** `lane-log.jsonl:237` records "clippy exit 0"
for the merged tree at `549f020`. That reading was taken with `cargo clippy --workspace --all-targets -q`
— **no `-D warnings`**, so it could not fail on a lint. CI runs `cargo clippy --all-targets -- -D warnings`
at the declared floor (`Cargo.toml:59`, `rust-version = "1.96.0"`), where `nonminimal_bool` fires.

**Not reproduced locally, and the cost that decided it:** this box has `clippy 0.1.98` / `rustc 1.98.1`
and **no `rustup`** (verified: `which rustup` → not found). The floor lint cannot be run here at any
price short of installing a toolchain manager. The disposition therefore rests on the log text naming
file, line and expression, plus the tree at `HEAD`, plus CI green on every run since. That is
sufficient for a deterministic lint and I am naming the gap rather than papering it.

---

### Event B — `F-MACHINEREPLACED-EVENT-RACE` (runs 4, 5)

**What failed.** `machine_replaced.rs:291`, both runs, same assertion verbatim:

```
thread 'hits_dropped_is_zero_and_PRESENT_on_a_window_state_load_when_nothing_was_recorded'
  panicked at crates/oracle-aether/tests/machine_replaced.rs:291:5:
assertion `left == right` failed: expected exactly one emulator/machineReplaced in the stream, got 0: []
  left: 0  right: 1
```

`test result: FAILED. 9 passed; 1 failed` in both. `c86e1c3` (09-12 22:50 UTC) and `ea1dcb8`
(09-13 07:30 UTC) are two sightings of one intermittent defect — **one event, two runs**; `ea1dcb8` is
a docs-only commit, which is itself evidence the cause was not in either changeset.

**FIXED-SINCE `c6ed909`** (2026-09-13 07:26 −0400), a descendant of `ea1dcb8` (verified with
`git merge-base --is-ancestor`). This is a FIXED-SINCE with a **mechanism**, which is what the rule
demands: the server registers a subscriber when the connection's reader thread handles `initialized`,
a JSON-RPC notification that gets **no reply**. `handshake(true)` sent it and returned immediately; the
test then gestured over an in-process mpsc channel with no socket in between, and `Engine::emit`
broadcast synchronously — so if the gesture beat `subs.add`, the event was never queued for that
connection. Nothing was *dropped*, which is exactly why `droppedEvents` was 0 and the symptom read as
`got 0` rather than as a lost event.

**Verified on the tree, not from the commit message.** The fix is a registration barrier, and it sits
on the precise path the failing test takes:

- `crates/oracle-aether/tests/common/mod.rs:646` — `handshake` calls `self.registration_barrier()`
- `:652` — `registration_barrier` issues `emulator/status` and correlates the reply id
- `machine_replaced.rs:245` — `fn attach(w) { ...; c.handshake(true); c }`
- the failing row's first two lines are `Window::start("zero")` then `attach(&w)`

So the failing test reaches the barrier through `attach`. Row 2's assertion site at
`the_one_replacement` is unchanged and still asserts exactly one.

**Local re-run:** `machine_replaced` at `HEAD`, 5 consecutive runs, `10 passed / 0 failed` each. I am
reporting that as **consistent with the fix, not as proof of it** — the defect was intermittent and
five green runs cannot distinguish a fix from luck. The weight is carried by the mechanism and by the
barrier being demonstrably on the path.

**What I did not do, and why.** `c6ed909`'s repro needs a timing mutation on disk (a sleep before
`subs.add`) and a 20-run loop per arm. Re-running it would be a second parcel's worth of work, which
this row forbids, and it would re-establish a mechanism already measured with a control
(300 ms *after* `subs.add` → 20/20 green, which is the arm that rules out "a slow reader thread").

---

### Event C — M13, the reused inode (runs 6, 7, 8)

**What failed.** `server::lifecycle_tests::a_dead_emulator_thread_releases_the_socket_and_reports_why`,
identically in all three runs:

```
thread 'aether-engine'   panicked at crates/oracle-aether/src/server.rs:1339:13:
  injected emulator-thread fault (lens M13 fixture)
thread '...releases_the_socket_and_reports_why' panicked at crates/oracle-aether/src/server.rs:1369:9:
  dropping the dead server's handle unlinked the socket the restarted server is serving on
```

`test result: FAILED. 93 passed; 1 failed` in all three. **One event, three runs** — `adcf239`
(09-11 21:25 UTC), `85c1599` (09-11 23:47 UTC), `82e812a` (09-12 00:11 UTC). Not a pair: a **triple**,
and the brief's clustering note stopped one short here too.

**FIXED-SINCE `d0844ce`** (2026-09-11 20:26 −0400 = 09-12 00:26 UTC — after all three runs).

**This one I proved rather than read.** `d0844ce`'s message asserts the mechanism and claims a repro at
`82e812a`; RULE 1 says that sentence is the claim under test. I built `82e812a` in a detached worktree
and ran the differential myself:

| arm | tree | `TMPDIR` filesystem | result |
|---|---|---|---|
| control, pre-fix, ext4 | `82e812a` | `/var/tmp` (ext4) | **RED 3 of 3** |
| control, pre-fix, tmpfs | `82e812a` | `/tmp` (tmpfs) | **GREEN 3 of 3** |
| post-fix, ext4 | `52178b8` (main) | `/var/tmp` (ext4) | **GREEN**, 4/4 legs |

The red arm's message is the CI message character for character, at `server.rs:1369`. Filesystems
confirmed with `df -T`: `/tmp` is `tmpfs`, `/` and `/var/tmp` are `ext4`.

**Mechanism.** `ServerHandle::shutdown` compared the socket's `(dev, ino)` *after* the accept thread —
and with it the listener — had gone, which is precisely when ext4 is free to hand that inode number to
the next socket bound at the path. A restarted server could therefore be handed the dead handle's
recorded identity and have its live socket unlinked. `d0844ce` moves the claim into a `Listening` value
owned by the accept thread, whose `Drop` runs **before** the listener field closes, so the comparison
is made while the inode is still pinned. Present at `HEAD`: `crates/oracle-aether/src/server.rs:209`,
`pub(crate) struct Listening`. `b185154` (09-12) extends the same mechanism to the hosted path.

**Note what the tmpfs arm means.** The defect was invisible on this machine for as long as anyone tested
on `/tmp`, and green there **for the wrong reason** — tmpfs does not reuse inodes the same way. A green
local run was not evidence of correctness; it was evidence of the filesystem.

---

### Event D — the panel masked-digit comparison (run 1)

`462e9cf` / `35418036061`: `panel_attribution::tests::a_run_on_no_reported_row_is_in_neither_string_and_changes_nothing`
FAILED, `519 passed; 1 failed; 4 ignored`. The later run `35418190314` on `2e9821b` shows the **same
test, same counts (519/1/4)** — they are one defect on two SHAs, and `2e9821b` is one of the two runs
the brief had already marked explained.

**REAL → FIXED-SINCE `1235148`**, tonight. The link is explicit and checkable from both ends:
`1235148`'s own message names *"CI run 35418036061"* as the instance, and `462e9cf` is an ancestor of
`1235148` (`git merge-base --is-ancestor` → yes). The digit mask removed a digit's *identity* but not
its *count*, so it broke when the measured fps crossed an order of magnitude on a slower runner; fixed
at the clock (`fixture()` drives from its own captured `t0`), and the mask was removed entirely.

**This row's disposition is "already explained" — and the finding is that the survey did not know that.**
`35418036061` was diagnosed, fixed and written up hours before this row was dispatched, in
`docs/OVERSEER-LOG.md:4013`, `docs/lane-log.jsonl:258` and `docs/2026-09-17-cr-w-landing.md:529`. It
entered the list of unchased reds because the survey enumerated runs and did not cross-check the
repo's own record of them.

---

## Rows for the board

**No REAL-and-live defect was found — all four events are fixed in the tree.** So there is no fix row
to book. What the investigation surfaced instead are two method defects, both of which caused this row
to exist:

> **F-CI-SURVEY-UNVERIFIED** — a CI survey reported "logs gone" for three runs whose logs retrieve in
> full, and mis-read `test result:` counts as suite progress when a run's log concatenates all three
> jobs. Both errors point the same way: they make a red look unchaseable. Second occurrence of the
> first error on this task. *Remedy shape: any survey that reports absent evidence carries a positive
> control on the extractor, as this document does.*

> **F-RED-NOT-CROSS-CHECKED** — `35418036061` was enumerated as an unchased red while its diagnosis,
> fix and write-up were already in `lane-log.jsonl`, `OVERSEER-LOG.md` and a dated doc. A red's
> disposition should be looked up in the repo's own record before it is called unknown. *Cheap remedy:
> grep the run id across `docs/` first; it cost one grep here.*

And one sized follow-up, **not dispositioned in this row**:

> **F-REDS-0909-COHORT** — seven further `failure` runs sit just outside the brief's ten-run window,
> all on 2026-09-09: `627295f`, `28e3c90`, `8b22de4`, `8cd6f2a`, `de95672`, `35f81ca`, `8a25969`.
> **All seven logs retrieve** (1318–2116 lines). Five exit 101, two exit 100 (the exit-100 pair is a
> different failure shape and worth separating). One names a test outright:
> `every_declared_bound_is_refused_by_name_one_step_outside` in `8a25969`. Cost to disposition, by the
> measure of this row: log-only, no builds, roughly the same effort as Events A/B/D combined. **Booked,
> not started** — dispositioning it here would have been the second parcel this row was told not to open.

---

## What this row actually establishes

The hit rate the brief argued from was 2 for 2. **It is now 6 for 6** on investigated reds (the two
from tonight plus the four events here), and **10 for 10** counting every run. Not one red in this
corpus was noise.

The eight were not undiagnosable. Three of the four events were *already* diagnosed in the tree — two
by the sessions that caused them, one tonight — and the fourth (Event A) was a deterministic lint whose
error message names its own fix. What was missing was never the evidence. **What was missing was
anyone looking, and a survey that said the evidence was gone made sure nobody did.**

RULE 2 was never reached: no red here had to be left UNDETERMINED, because none of them resisted a
first look.
