# Disk cleanup, 2026-09-06 — oracle's share

**Owner's word, relayed by the hub (empyrean-c2), ~15:0xZ:** *"can we get a cleanup of the board and
then a cleanup of the worktrees or whatever all this is"*. This file is the manifest half; the board
prune is recorded separately in `docs/lane-status.json`'s queue and `docs/OVERSEER.md`.

**Scope discipline.** The hub assigned this lane its `.oracle-*` prefix. Everything below was verified
to be this lane's before it was touched, and two things outside the prefix are named at the foot rather
than acted on: **a lane deletes its own dirt, never a peer's.**

Free space before: **490 G available of 1.8 T (72 % used)**.

## Method, and the two measurements that changed the answer

**Live-holder check.** The first pass reported three processes holding the target dirs. It was wrong:
the grep's own argv contained the pattern, so the pipeline matched *itself*, and the PIDs were gone by
the time they were read. The re-run used a pattern that cannot match itself and a positive control
(`claude`, 36 processes matched), which is what makes the zero meaningful — this repo's own standing
lesson that a decorated tool's failure is indistinguishable from its empty result. **Zero live holders,
measured against a control that proved the instrument could see.**

**Merge check before removal.** Every worktree was checked for *both* uncommitted changes and whether
its HEAD is an ancestor of `origin/main`. All nine: `uncommitted=0`, `inMain=YES`. Branch refs are left
in place — removing a worktree does not delete its branch, so nothing became unreachable.

## Deleted

| Name | Size | Age | Reason |
|---|---|---|---|
| `oracle/.claude/worktrees/agent-a03582eea0b9aff4f` | 12 G | landed | branch `aether-bind-failure-on-glass`, `539927f`, in `main`, clean |
| `oracle/.claude/worktrees/agent-a0b544fd12f8a2c76` | 13 G | landed | `d63d561`, in `main`, clean |
| `oracle/.claude/worktrees/agent-a2beadefa66c1c846` | 5.9 G | landed | `1b722ec`, in `main`, clean |
| `oracle/.claude/worktrees/agent-a3e7f50de4af29ef7` | 3.4 G | landed | branch `phase-table-parse`, `b8aee07`, in `main`, clean |
| `oracle/.claude/worktrees/agent-ab184b39bf1bf7b0e` | 23 M | landed | `0199609`, in `main`, clean |
| `oracle/.claude/worktrees/agent-abfa2aed01c82a63b` | 5.8 G | landed | `0b010de`, in `main`, clean |
| `oracle/.claude/worktrees/agent-acf8095ab60278bab` | 8.8 G | landed | `24197d5`, in `main`, clean |
| `oracle/.claude/worktrees/agent-ad871d58626e8f705` | 9.5 G | landed | `e6bb095`, in `main`, clean |
| `oracle/.claude/worktrees/agent-ae0c6840fedc59f98` | 6.5 G | landed | branch `planes-panel-pick`, `91a316f`, in `main`, clean; was `locked` and unlocked first |
| `.oracle-pp-target` | 6.8 G | 2026-09-02 | cargo target dir (`CACHEDIR.TAG`), no live holder |
| `.oracle-sp-target` | 6.3 G | 2026-09-02 | cargo target dir (`CACHEDIR.TAG`), no live holder |

**Worktrees 63.0 G + target dirs 13.2 G = ~76 G.**

### The 536 KB inside `.oracle-pp-target` that was not build output, named because deleting evidence quietly is not housekeeping

`.oracle-pp-target` also held diagnostics from the parcel that used it: five PNG captures
(`pp.png`, `pp-toast.png`, `pp-va3.png`, `pp-env.png`, `pp-toast-zoom.png` and two `*-status.png`),
`xvfb.log`, `clippy.log`, four `player*.out`/`player*.err` pairs, two launch scripts, and
`item-e-ran-relabel.patch`. Total 536 KB. **Deleted with the rest, on these grounds:**

* **Nothing in `docs/` or `crates/` references any of them** (grepped; no hits).
* **The patch is superseded, not unlanded.** It relabels the window's frame tally to `RAN n`. `main`
  ships `DRAWS n` (`overlay.rs:752` and five test rows), and the patch no longer applies to any of its
  five files. So it is an earlier draft of a relabel that shipped in a different spelling, not work in
  danger of being lost. *(The first verdict printed here said the patch applied cleanly; that was a
  shell error — `head` in the pipeline replaced git's exit status. The real result is that it does not
  apply.)*

## Kept

| Name | Size | Reason |
|---|---|---|
| `oracle/target/` | — | the live build; holds the `0d6180c` binaries the owner's window is running |
| every `worktree-agent-*` / named branch ref | bytes | removing a worktree does not delete its branch; the history stays reachable |

## Not mine — named, not touched

* **`.agent-targets`, 32 G — the single largest directory in the workspace, and it belongs to sigil.**
  Its `stamp.txt` names `crates/sigil-frontend-emp/tests/cond_branch_witnesses.rs` and its
  `run_suite.sh` sets `cd /home/volence/sonic_hacks/sigil/.claude/worktrees/agent-ae5fd184616b5bff7`.
  ⚑ **It falls outside the `.sigil-*` / `.cargo-target-*` prefixes the hub assigned sigil**, so on the
  prefix split as issued it belongs to nobody and would have survived a cleanup everyone reported as
  complete. Reported to the hub for routing to sigil.
* `.sigil-*`, `.cargo-target-*`, `.target-shared-target-parcel` — sigil's, per the hub's split.
* `.aeon-*`, `.parcel-*` — aeon's, per the hub's split.
