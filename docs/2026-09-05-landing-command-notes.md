# Notes for LANDING-COMMAND, gathered before it is built

Approved by the hub 2026-09-05, sequenced after the plane viewer and before the data-display audit,
on the grounds that every landing from here rides on it. These are findings from the evening that the
parcel should not have to rediscover.

## Why it exists

Checked, not recalled: this repo has **no landing script, no task runner, and no CI**. Six narrow
helpers under `tools/` fetch fixtures or run one suite each. Every landing has run a set of checks
chosen by hand at the time. Sigil had a named script and drifted off it for eleven parcels, which is
how two faults reached its main copy including a clippy red that sat there for an afternoon. Our
position is worse: there is no definition to drift from.

## The definition, as ruled

Workspace release suite, fmt check, clippy all-targets, the doc bound gate, the schema conformance
gate, and the vendor symlink precondition.

Two of those are **already workspace test targets**, so `cargo test --workspace` runs them:
`crates/oracle-aether/tests/overseer_bound.rs` and `crates/oracle-aether/tests/schema_conformance.rs`.
They do not need separate invocation. What they need is proof they ran, which is the next section.

## Aeon's addition, and it is the load-bearing one

**A leg count that must equal an expected number, so the script proves it ran to the END and not merely
what ran.** Aeon's evidence: a matching md5 on a run that never finished was the more convincing
artifact of the night. Ours matches. Twice tonight a full suite was killed partway and its log
aggregated clean; the second time it reached 56 legs of 75 with zero failures, which is indistinguishable
from success in every summary except the leg count.

## The trap in that number, found while writing these notes

**Do not hardcode 75, and do not trust the naive derivation either.** Counting lib, bin and test targets
from `cargo metadata --no-deps` gives 69, plus one doc-test leg per package with a lib target (4) = **73**.
The measured run has **75**. That gap of 2 is UNEXPLAINED and is the parcel's first job. Resolve it before
writing the assertion, because an expected count that is wrong in the safe direction is a gate that can
never fire, and one wrong in the other direction is a gate nobody can land through.

Per-package targets as measured: oracle-aether lib=1 bin=1 tests=42; oracle-core lib=1 tests=16;
oracle-frontend lib=1 bin=1 tests=1; oracle-player bin=1; oracle-replay lib=1 bin=1 tests=2.

## The precondition a checklist would keep omitting

A fresh worktree has no `vendor/` symlink. Without it, twelve `save_state` rows fail and the
SingleStepTests sweep **skips and passes vacuously**. Its failure mode is a silent green, which is
precisely what a human-read checklist is worst at catching and what a script is best at.

## Two of ours in the same family, for the record

A full suite read from chunked runs whose totals agreed for cancelling reasons (feature unification
between `-p X -p Y` and `--workspace` dropped 57 synth tests one way and profile differences moved
ignored counts the other). And a scoped gate accepted on argument rather than on a run.

## Shape

Build it, not a checklist. A checklist is the artifact that drifts; that is the whole lesson from sigil.

## Aurora's spec, adopted (from `npm run land`, aurora 181d4397)

The framing that carries it: **the pushed tree is never the tested tree by construction** whenever a
lane-log or notes commit follows the suite. The command must therefore:

- **(a)** refuse a dirty tree before anything runs, and **list the paths** rather than just saying dirty.
- **(b)** push nothing on a failing suite, and **verify afterwards that the remote has no such ref**.
- **(c)** refuse if HEAD moved under the run.
- **(d)** read the remote SHA before and after and **say in words whether the push did anything**, because
  `git push` exits 0 on already-up-to-date, so its exit code cannot distinguish "pushed" from "did nothing".
- **(e)** **push the TESTED SHA by name, never the branch tip.**

It does not merge, does not commit, and does not write the lane log. Those stay deliberate acts.

### A live instance of (c), caught by the spec arriving mid-run

While the plane-viewer suite was running against merge `63f5301`, this very notes file was committed on
top of it, moving HEAD to `e0ff771`. The content is docs-only and cannot affect a Rust suite, which is
exactly the reasoning that makes the habit feel safe and is not the point: **had the suite gone green I
would have pushed a tip that no run had ever tested**, and nothing in my procedure would have said so.
Recorded here rather than tidied away, because it is the same defect the spec describes, committed by the
person writing the spec down, four minutes after being told about it.

This is also why (e) is the operative clause rather than (c). Refusing on a moved HEAD makes the failure
loud; pushing the tested SHA by name makes it **impossible**, and only one of those survives being tired.
