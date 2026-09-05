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

---

## BUILT — `tools/land.sh`, 2026-09-05

The command is `./tools/land.sh`. Its own header is the reference; this section records only what these
notes got wrong or left open.

### The gap of 2 is RESOLVED: two examples that opt into `cargo test`

`crates/oracle-core/Cargo.toml` declares

```toml
[[example]]
name = "motion_run"
test = true

[[example]]
name = "ab_compare"
test = true
```

so those two example targets are run by `cargo test` exactly like any other target and emit a leg each.
73 + 2 = 75, which is the measured number, and the reason the naive derivation missed them is that it
counted only lib / bin / test kinds. `cargo metadata` reports all 16 of `oracle-core`'s examples, but only
these two carry `"test": true`.

**The script does not encode that.** Patching a headcount with an examples clause only works until the
next surprise is not an example, so `land.sh` asks cargo instead: the runnable legs are the distinct
executables in `cargo test --workspace --release --no-run --message-format=json` with
`profile.test == true` (71 today — cargo's own target selection *and* its own `required-features`
resolution), and the doc-test legs are the lib targets with `doctest == true` in `cargo metadata --no-deps`
(4 today), which have no executable and are therefore the one part cargo will not hand over as an artifact.
71 + 4 = 75, derived, self-updating when a crate or a test file is added.

### Two corrections to these notes

* **"No CI" is not true.** `.github/workflows/ci.yml` is tracked and defines three jobs (determinism gate;
  fmt / clippy / fetch / `cargo test --workspace` in **debug**; and the release replay playthroughs via
  `tools/replay_playthroughs.sh`). What is true is that nothing local ever consulted it, that it gates the
  push rather than preceding it, and that its main suite runs the debug profile. It also means the suite
  already carries six `CI`-armed vacuity guards — four named `vendor_data_present_when_running_in_ci`
  tests (`conformance_roms`, `scanline_goldens`, `singlestep_m68000`, `singlestep_z80`) plus two inline
  "skip locally, never under CI" refusals (`oracle-aether/tests/scanlines.rs`,
  `oracle-core/tests/scanline_capture.rs`). `land.sh` exports `CI=1` and arms all six, which is a stronger
  vendor precondition than any path check the script could hand-write, because they assert against the
  test files' own ROM and opcode manifests rather than against a directory being non-empty. Measured:
  `cargo test --workspace --release -- vendor_data_present_when_running_in_ci` with `CI=1` is
  `4 passed, 0 failed` over 75 legs.
* **A clippy gate without `-D warnings` cannot fire.** The definition as ruled says
  `cargo clippy --workspace --all-targets --release`, which exits 0 on every lint it reports. The script
  runs it with `-- -D warnings`, matching what the repo's own CI already denies.

### The dirty-tree carve-out

Exactly one tolerated path, matched as a literal string and always printed rather than skipped silently:
`docs/lane-status.json`. Safe because nothing compiled reads it (`git grep lane-status` over `*.rs` /
`*.py` / `*.sh` is empty) and because (e) pushes a commit by name, so its working-tree content is never
what reaches the remote. Every other path, tracked or untracked, still refuses.

### One ordering fact, measured rather than guessed

`cargo clippy` and `cargo test` disagree about the workspace crates' fingerprints, so whichever runs
second recompiles them. The first red-first run had the leg-count derivation before clippy and G7 then
re-compiled `oracle-aether`, `oracle-frontend` and `oracle-player` that clippy had just built. The gates
are therefore ordered fmt → clippy → leg-count → suite, so the `--no-run` build is the one the suite
reuses. (Only the workspace crates churn; the dependency graph is shared, so a separate
`CARGO_TARGET_DIR` for clippy would cost ~1.7 GB to save what re-ordering saves for free.)
