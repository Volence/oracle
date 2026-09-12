# Brief: F-PLANES-CHOICE-ROW-UNBOUNDED — the Planes tab's choice row has no ceiling

**Row:** `F-PLANES-CHOICE-ROW-UNBOUNDED` (S), the board's `next` at dispatch.
**Branch:** `parcel/planes-choice-row`. One agent, one worktree.
**Size:** small by intent. If it turns out not to be small, say so and stop rather than growing it.

## The defect, as it was booked

Its whole record is one lane-log line (`docs/lane-log.jsonl`, 2026-09-12T10:46:28Z, the
`F-TWO-SPELLINGS-OF-ONE-GIVEUP` landing), quoted rather than paraphrased:

> Found: egui_dock's tab ScrollArea floors at 64 pt, so the Screen give-up is defensive only; booked
> F-PLANES-CHOICE-ROW-UNBOUNDED (the Planes choice row wraps past the floor in 80-200 wide, 30-90 tall
> panes). TAG: Planes sentence now top-left, not centred.

**That measurement is the previous agent's, not this seat's, and it is a floor on the problem rather
than a specification of it.** Re-measure the regime before you fix it: the pane sizes at which the
choice row actually costs the picture its room are yours to establish, and the numbers above are where
to start looking, not expectations to reproduce.

## The sites

- `crates/oracle-player/src/ui.rs:1509` `Panels::planes` — the `ui.horizontal_wrapped` block at
  `:1512-1538` is the choice row: the `planes::CHOICES` selectable labels, a separator, the `viewport`
  checkbox and the `apply live scroll` checkbox. It wraps, and nothing bounds what it takes.
- `crates/oracle-player/src/ui.rs:3044` `plane_split` — the two-column split below it (side column +
  picture) that gets whatever height the choice row leaves.
- `crates/oracle-player/src/ui.rs:290` `NO_ROOM_FOR_PICTURE` — the one sentence both picture tabs say,
  and its doc comment already names this defect in prose: *"On the Planes tab that is the row of plane
  choices above `plane_split`, which wraps in a narrow pane"*.
- `crates/oracle-player/src/ui.rs:414` `SCREEN_STRIP_MAX_SHARE` / `:423` `screen_strip_cap` / `:508`
  `screen_strip` — **the sibling fix, already landed**, for `F-SCREEN-TAB-STRIP-UNBOUNDED`. Read all
  three doc comments in full before designing: they carry the reasoning for the shape, for one half
  rather than a tuned fraction, for `min_scrolled_height(0.0)` against egui's 64-point floor, for
  `auto_shrink([false, true])`, and for why a scroll area over the whole tab is the wrong cure.
- `crates/oracle-player/src/ui.rs:8070` `screen_strip_tests` and `:7568` `no_room_tests` — the headless
  harness and the gate shapes to mirror. `PANE_HEIGHTS` at `:8085` is the existing height sweep.

## What to decide, and it is a design call rather than a transcription

The obvious move is to give the choice row the same ceiling `screen_strip` gives the control strip.
**Take that as the floor, not the ceiling** (this lane's standing rule: a legacy or sibling shape is
the compatibility floor, never the design ceiling — run a visible better-approach pass and say what you
rejected). Two things make the Planes case genuinely different from the Screen case, and the brief does
not prejudge either:

1. **The strip is text; this row is controls.** Scrolling prose is ordinary; a control scrolled out of
   view is a control a person cannot find. The house rule that bears on it is in this same file at
   `:1529` — *"a control that vanishes teaches nothing"*, which is why `apply live scroll` is disabled
   rather than hidden on the window plane. Whether a capped scroll area satisfies that rule for a row
   of controls is the call to make and to justify, not to assume. Alternatives worth pricing before you
   pick: cap-and-scroll (the mirror); a row that stops wrapping and scrolls across; moving part of the
   row into `plane_side_column` when the pane is narrow. Pick one, state the alternatives and why they
   lost, and keep the change small.
2. **Two spellings of one rule is the defect this repo just spent a parcel closing.**
   `F-TWO-SPELLINGS-OF-ONE-GIVEUP` merged one week's worth of exactly that. If your fix ends with the
   Screen tab and the Planes tab each carrying their own copy of "a control area may not take more of
   the pane than it leaves for the picture", you have created the sibling of the defect you were sent
   to fix. **Prefer one implementation under two consumers plus a parity row** — that is this repo's
   stated way of making parity true by construction (`ui.rs:7036` and `host.rs:439-440` carry the
   reasoning). If that means renaming `SCREEN_STRIP_MAX_SHARE`/`screen_strip_cap` to names that are not
   Screen-specific, that is in scope; if you judge the two cases genuinely different enough that one
   helper would be a false unification, say so explicitly and keep them apart on the merits.

**Out of scope, and do not drift into it:** what the choice row *looks like* — no row added, removed,
reordered or restyled. The owner has a parked look call on the Screen strip's appearance
(`docs/2026-09-09-palette-shape-and-the-strip.md` §2) and the same boundary applies here: appearance is
his, fit is ours. Also out: the Planes give-up sentence's placement (the TAG above, top-left rather than
centred) — note it if you touch that code, do not fix it.

## Hazards — stated as unverified inference, because that is what they are

The last four agents in this repo each corrected their brief, and the last one corrected a hazard I
had stated as fact. So: **the following are my reasoning, not measurements. Check each before relying
on it, and tell me which are wrong.** "First, tell me where this brief is wrong" is load-bearing here.

- I expect egui's 64-point `ScrollArea` floor to bite the same way it did on the Screen strip, so a cap
  without `min_scrolled_height(0.0)` would be inert in exactly the panes this row exists for. Verified
  only to the extent that `screen_strip`'s doc comment says so at `:505-507`, citing
  `egui-0.36.1/src/containers/scroll_area.rs:399` applied at `:776`. I have not re-read egui's source.
- I expect the choice row's wrapped height to depend on pane **width** (more wrapping when narrower)
  while the cure is about **height**, which is why the booked regime is a width-and-height box rather
  than a height threshold. If that is wrong the whole framing may be wrong; say so.
- I do **not** know whether `plane_split` or `plane_image` already absorbs part of this — the give-up
  sentence appearing at all means something already detects the squeeze. Establish what today's build
  actually does at those sizes before changing it, and report the before/after.

## Gates

Any check you add meets all five clauses of the invariant block below, and in particular:

- **Derived, not copied.** The bound's expectation comes from the constant and the pane height with the
  derivation shown, never a number typed from `screen_strip_tests`.
- **Vary the mutation parameter.** This repo has twice shipped a guard that was green under the one
  mutation its author tried (`F-PARITY-BLIND-TO-SAT-STRIDE`; and the 16-bit DMA counter whose width was
  unguarded because every pinned case answered the same under 15 and 16 bits). Mutate the share, the
  floor, and the cap's application site separately, and say which mutations each row catches.
- **An assertion against the constant under test is circular.** A gate comparing the cap to
  `SCREEN_STRIP_MAX_SHARE * pane` stays green when the constant moves. Pin the *property* — the control
  area is not the larger half — not the arithmetic.

## Baseline handed to you

At `origin/main` `760f832`, the previous session's `land.sh` reported **release 2891 passed / 0 failed /
3 ignored**, legs **89/89 by both counting methods**. That figure is **theirs, read from their land log,
not re-measured by me** — re-derive it in your worktree before you trust the delta, and report your own
numbers as totals rather than a tail.

`./land.sh --no-push` is the landing gate; run it detached and poll your own log file for its
`LAND-EXIT=` / `land: RED|GREEN` token. **The token in the log is the verdict — never a harness exit
code, never a notification.** Do not edit any file in the tree while it runs: gate G10 fails the whole
run on a tree modified mid-run, including a docs edit, and it is right to.

## Deliverable

A report naming: what changed (files, branch, each commit SHA, your tip SHA recorded before you stop);
the design call you made and the alternatives you rejected, with reasons; what you measured about
today's behaviour at the affected pane sizes, before and after; each gate by name with its mutation
shown applied on disk and the red run quoted; aggregate test totals; and **everything in this brief you
found to be wrong.**

---

## Invariants (adapted from `dispatching-empyrean-agents`; none of these are optional)

1. **No emulator, ever.** Never touch `mcp__oracle__*` / emulator MCP tools — they deadlock from
   background agents. A finding wanting runtime confirmation is TAGGED for my foreground follow-up,
   never attempted.
2. **Branch/tree discipline.** Work only in your own worktree on `parcel/planes-choice-row`. Never
   commit to `main`. Verify the branch at commit time; other sessions share this tree.
3. **Exact-path commits.** `git add` enumerated paths only — never `-A`, never a glob. Verify each
   commit with `git show --stat`.
4. **Escape hatch.** If BLOCKED — a constraint that forces a worse design, an unclear ownership call,
   a missing fixture — STOP on that item, record exactly why, and continue with the rest. Never
   silently degrade the design to reach green. Autonomy covers open calls with defensible answers;
   BLOCKED covers constraint conflicts.
5. **Faithful reporting.** Aggregate totals with failing names, never a tail excerpt. If something
   fails, say so with the output.
6. **Commit before any long run; a death must cost the run, never the work.** Write the finding INTO
   the commit message — if you die mid-parcel your commit bodies are the only surviving record. `wip`
   loses exactly what this rule protects.
7. **Long runs:** foreground where they fit your cap; otherwise detach and poll a log file you wrote
   for an end marker you wrote. Never end a turn waiting on a background-task notification — it may
   never reach a subagent. A killed or capped run is never a pass, and a missing result file means
   "did not run", never a finding.
8. **A missing branch or worktree after landing is the EXPECTED end state.** When this lands I merge
   your branch and delete both. Record your tip SHA while you still have a ref to it; if you ever need
   to check, `git merge-base --is-ancestor <tip> main` — and note a non-ancestor result is ambiguous
   (a squash rewrites commits), so confirm by content.
9. **Deliverables are committed.** A report that exists only in the transcript does not exist to other
   sessions.
