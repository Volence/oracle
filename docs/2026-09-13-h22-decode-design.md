# H22: should 68000 decode remember its answers?

**Design call, docs first.** Parcel H22-68000-DECODE (lens finding `H22`, seat P2, 2026-09-06). Measured on
branch `worktree-agent-abb6d27d944092d4d` from `main` at `d77182c`. The measurement spike is on the branch
(commits labelled `spike:`, feature-gated, **not proposed for merge**; §10).

## Summary

The CPU's decoder turns each 16-bit instruction word into a list of steps. The question was whether it
should remember those lists instead of rebuilding one for every instruction it runs. **Yes.** Decoding is
about a third of all emulation time on real Sonic 4 gameplay, and remembering answers makes the whole
emulator **15-18 % faster**. It is **22 % faster** if the two branch-instruction families get the same
treatment. The finding rested on one claim, that the decoder looks only at the instruction word. **That claim
is false:** for about a tenth of all instruction words (branches, loops, some shifts, bit operations and
register saves) the decoder also reads the CPU's flags or registers. Those words are about 40 % of the
instructions actually executed. **I recommend a small cache (a few MB, not the 66 MB a full table would
take) in front of the unchanged decoder.** It would learn which words are safe to remember from the
decoder itself, not from a hand-kept list. It ships with a one-line change to how step lists are built,
worth 7.5 % on its own, and with exhaustive tests that fail the moment a remembered answer could differ from
a freshly built one.

**Recommendation: option E\*** (§7): a lazy per-opcode memo in front of the unchanged cascade, filled by
a register-free pass of the cascade itself, plus the static-copy filler, plus fact-keyed entries for Bcc and
DBcc. Staged as three landings. **M32** (reorder hot arms) should close as dissolved: the arm walk has no
measurable cost (§8).

## 0. Where the brief and the finding were wrong (measured)

1. **"The whole cascade reads only the opcode and one supervisor bit" is false.** `decode_dispatch` hands
   `regs` to eight builders, and seven of them read it: `d0`-`d7`, `prefetch[1]`, and the CCR bits N, Z, V
   and C. 12 594 of the 131 072 `(S, opcode)` keys (9.6 %) depend on more than the key. Under real load
   those keys are **39-41 % of executed decodes**, because Bcc, DBcc and MOVEM are hot. So decode is
   memoizable for about 90 % of the key space and about 60 % of executed instructions, behind a purity
   partition, not "fully" (§1).
2. **The cost is not the 104-arm walk.** MOVEQ sits at arm 101 and decodes as fast as `MOVE.w` at arm 2,
   within run-to-run noise (19.0/26.7/27.5 ns vs 24.7/25.8/25.3 ns across three runs). The expensive part
   is *materializing* the 532-byte recipe. `RecipeBuf::new` + one push + `finish` alone costs 25-29 ns,
   about one whole decode (§2.3). The finding's framing ("a 104-arm cascade") points at the wrong cost,
   and so does M32.
3. **The key does not need the S bit.** Only 86 opcodes decode differently with S=0 and S=1, and they are
   exactly `is_privileged_opcode`'s 86 (0 mismatches). The gate is the only reader of S. With the gate kept
   in front, the key is the opcode alone, which halves any table (§6).
4. **The seat's reason for rejecting a generated table (it loses the per-arm recon citations) is not the
   decisive one.** A generated table is an output of the cascade, and the citations stay on the cascade.
   What sinks it is drift between regenerations and a roughly 60 MB literal (§4, row D).
5. `MicroState` is `Clone`, **not `Copy`**. Every field is `Copy`, so a clone is a 532-byte block move
   (2.6-4.0 ns measured).
6. Minor: MOVEQ's arm is at `decode.rs:1225`, not `:1223`. The 104 count holds, with the privilege gate
   counted as arm 1.
7. Not the brief's claim, but stale nearby: `replay_real_artifacts.rs`'s docs give release playthrough times
   of about 4 s and 6 s. Measured today: 1.50-1.53 s and 2.04-2.09 s.
8. **A measurement trap I fell into and corrected.** My first A/B switched variants at run time. The switch
   itself costs about 7 %: the no-feature build ran the standing playthrough in a median 1.55 s, against
   1.66 s for the feature-on "cascade" mode, over 3 reps with non-overlapping ranges. Every headline number
   below therefore comes from **compile-time variants against a no-feature build**. The runtime-switch
   numbers are kept only as supporting rank order.

## 1. Q1: is decode a pure function of (opcode, S)?

**Verdict: no. It is a pure function of (opcode, S) for 118 478 of the 131 072 keys. For the other 12 594
it also depends on CCR N/Z/V/C, `d0`-`d7` and `prefetch[1]`, and on nothing else.**

### 1.1 By what touches the data (per-path)

`decode(regs: &Registers)` has one input. Every builder that can see registers is reached from
`decode_dispatch` by passing `regs` by name (`grep regs` inside the dispatch: 25 call sites, 8 builders).

| path (symbol) | takes `regs` | what it reads | only when | keys (per S half) |
|---|---|---|---|---|
| privilege gate (top of `decode_dispatch`) | yes | S via `Registers::supervisor()` | privileged opcode in user mode | 86 opcodes (keyed away, §6) |
| `bcc_recipe` (arm 75) | yes | CCR through `condition_true` (taken or not) | cc ≠ T (BRA is pure) | 3 584 |
| `shift_recipe` (8 arms from 102) | yes | `d[ccc] & 63` (the idle cycle count) | register form, Dn count (bit 5 = 1) | 1 536 |
| `scc_recipe` (arm 83) | yes | CCR (the value written, and the +2 idle) | cc ∉ {T, F}, legal EA | 700 |
| `bit_recipe` (BCHG/BCLR/BSET) | yes | `d[n]` (dynamic) or `prefetch[1]` (static): the `pos ≥ 16` +2 idle | Dn destination only | 216 |
| `movem_recipe` (arm 72) | yes | `prefetch[1]`, the register mask (it expands the list) | legal EA | 140 |
| `dbcc_recipe` (arm 82) | yes | CCR + `d[reg] & 0xFFFF == 0` (expired) | cc ≠ T | 120 |
| `trapv_recipe` | yes | V | always | 1 |
| `movep_recipe` | yes (`_regs`) | nothing | — | 0 |
| the other ~95 arms' builders | no | opcode only | — | 0 |

Per half that totals 6 297, so 12 594 over both halves. The code's own comments name the mechanism:
these builders resolve a data dependency **at decode time** ("like Bcc/DBcc/TRAPV"), so the recipe stays a
flat linear list that both drivers and mid-instruction snapshots can walk.

### 1.2 Varying the axis (what a `grep regs.` would miss)

- **Other inputs.** `decode` takes only `&Registers`. There are no statics, thread-locals, `Cell`s,
  atomics, `env::var` or clock reads anywhere in `m68000/` non-test code: `grep` over `decode.rs`, `ea.rs`,
  `microop.rs`, `exception.rs` and `registers.rs` finds only the phrase "static form" in doc comments.
  `Registers` is plain data with no interior mutability. The only `Registers` method the dispatch calls
  is `supervisor()`. The EA builders (`ea_src`, `ea_dst`, `ea_move` and the rest) take `RecipeBuf` plus
  mode/reg, never registers; `compute_ea` does take registers but is only called from `ea.rs` tests. The
  exception-frame builders take only the buffer. `condition_true(cc, sr)` is a pure function of its
  arguments. Closures passed to `ea_dst` capture `op`, `size` or `v`, never `regs`.
- **Dynamic probe (the spike, phase 1).** For every one of the 131 072 keys, from three base register files
  (all-zero, all-ones, mixed), each of 27 fields was perturbed: `d0`-`d7`, `a0`-`a6`, usp, ssp, pc,
  `prefetch[1]` (24 values each), every SR bit, I2-I0 and the unimplemented SR bits. The recipe was then
  compared. A joint-only check compared the three bases against each other. Keys whose recipe changes:
  `d0`-`d7` 460 each, `prefetch[1]` 328, C 2 512, V 3 770, Z 3 768, N 3 768. **Zero** for `a0`-`a6`, usp,
  ssp, pc, X, T, I2-I0 and the unimplemented bits; zero joint-only dependences. The impure key set is
  exactly the 12 594 of the table above.
- **Real load.** 3 733 764 table lookups on the frozen `s4.debug.bin` were compared against the cascade *on
  the live registers*, with no mismatch (spike mode `TABLE_CHECKED`). Every memo variant also left both
  replay playthroughs green, and ended the `s4.debug.bin` run at the same `export_state_hash`,
  `0xe80157513937dce6`.

### 1.3 The caveat, exactly

Purity holds **per key, behind a partition**. The partition is the risk: a key wrongly called pure would
serve one register file's recipe to every other register file. The partition must therefore come from the
cascade itself or be gated exhaustively. **The probe is evidence, not proof.** Reading its counts back
shows it under-counts `d0`-`d7` (460 each where the builders imply 462). DBGE's counter dependence is
invisible from all three bases, because GE is true at each of them. The *key* was still caught, through its
N/V dependence, so the set held. But a probe that happened to miss both would not have. That is why §7
makes purity a type-level fact rather than a probe verdict.

## 2. Q2: is decode hot enough to matter?

**Yes: about a third of emulation time on real gameplay.**

### 2.1 Method

Two real workloads, both release builds, run headlessly through the core (no emulator tools):

- the replay playthroughs `the_standing_fixture_runs_green` (OJZ, 1 824 frames) and
  `the_slide_fixture_runs_green` (2 662 frames), timed by the harness's own `finished in`;
- the frozen `fixtures/aeon/s4.debug.bin`, run by the spike example: 1 800 warm frames, then 600 timed
  frames from one cloned state, with a scripted pad.

Decode's share comes from **add-one**: a build that decodes twice and keeps the second. The extra time is
one decode's cost under real load. There is no `perf` or `valgrind` on this machine. Variants are
compile-time builds, run in rotating order, 3-5 reps each, reported as medians.

### 2.2 Numbers

| workload | main | double decode | decode share |
|---|---|---|---|
| standing playthrough (5 reps) | 1.53 s | 2.02 s | **+32 %** |
| slide playthrough (3 reps) | 2.04 s | 2.73 s | **+34 %** |
| `s4.debug.bin`, 600 frames (runtime switch, rank only) | 0.771-0.782 ms/frame | +26.5 to +29.6 % | — |

`uptime` at the time: standing load 2.20-3.95, slide 2.50-3.42 (16 cores, shared with other sessions).

Cross-checks:
- The real mix runs 10 205 decodes/frame on `s4.debug.bin` and about 11.5 k/frame in OJZ (the recorder's
  running count). At a measured 25-30 ns per decode in a tight loop over 765 393 recorded real decode
  inputs, that is 30-40 % of the frame; the tight loop streams 61 MB of samples, so it overstates.
- The existing `microop_perf` example on the SST mix gives decode-only 33 ns against full 80 ns per
  instruction (load 2.0-3.7).

### 2.3 Where the cost is

| measurement (tight loop, ns) | run 1 | run 2 | run 3 |
|---|---|---|---|
| `decode` MOVE.w D0,D1 (arm 2) | 24.7 | 25.8 | 25.3 |
| `decode` MOVEQ #1,D0 (arm 101) | 19.0 | 26.7 | 27.5 |
| `MicroState::from_ops(&[Prefetch])` | 22.6 | 21.8 | 21.2 |
| `RecipeBuf::new` + push + `finish` | — | 29.3 | 28.9 |
| the same with a static-copy filler | — | 24.9 | 25.0 |
| `MicroState` clone (a memo hit's copy) | 2.6 | 3.1 | 4.0 |

`MicroOp::Internal` is variant 4, so the `[Internal { cycles: 0 }; 40]` filler is not an all-zero pattern
and is built element by element. A control that called the builders directly came out 2-7 ns *slower*
than `decode` itself (one more 532-byte move through an `Option`). So the arm walk is below the resolution
of one struct copy. Tight-loop single-opcode numbers moved up to about 30 % between runs; the frame-level
A/B moved about 1 %. **Trust the frame-level numbers; tight loops only rank.**

### 2.4 Error and representativeness

- `finished in` resolves to 10 ms. Within a variant, reps spread ≤ 0.03 s, except load spikes that hit
  every variant in that rep (standing rep 2: main 2.75 s; a later slide rep 1: main 2.56 s). Medians absorb
  them.
- Both workloads are **one game that idles in a VBlank spin**: `tst.b abs.w` + `beq.s -6` are 54-60 % of
  executed instructions. Spin instructions are cheap to execute, so decode's share is at its highest here.
  A CPU-bound game would see a smaller share, though the absolute nanoseconds saved per instruction would
  not change. **TAG-1** (§11).

## 3. Q3: the cost of a table

| item | measured |
|---|---|
| `size_of::<MicroOp>()` / `MicroState` / `Option<MicroState>` / `Registers` | 12 / **532** / 532 (niche) / 80 B |
| `MicroState` | `Clone`, not `Copy`; clone = block move, 2.6-4.0 ns |
| eager table, key `(S, opcode)`, 117 326 pure entries | **66.5 MiB**; cold build **5.8-6.3 ms** (4.0 ms of it the decodes, the rest page faults) |
| eager table keyed by opcode only (gate in front) | 33.3 MiB (computed) |
| distinct pre-latch recipes among the pure keys | 36 848 → about 18.7 MiB deduplicated, plus an index |
| lazy memo (`OnceLock<Option<Box<MicroState>>>`) | 16 B/slot: 2 MiB for `(S, opcode)`, 1 MiB opcode-only, **+532 B per touched key** |
| keys touched | 666 on `s4.debug.bin`; about 1 600-1 700 in OJZ gameplay → 0.3-0.9 MiB of recipes |
| Bcc/DBcc fact-keyed memo | (8 192 + 384) slots × 16 B = 134 KiB, plus touched recipes |

## 4. Q4: the options on the same axes

Speed is compile-time variants against no-feature `main`, as medians: standing / slide. Two independent runs
of `lazy + filler` gave −15.7 %/−13.7 % and −15.3 %/−17.7 %, so read ±2 percentage points as noise.

| option | speed | memory | audit trail | determinism | test story | blast radius |
|---|---|---|---|---|---|---|
| **A** leave the cascade (M32's reorder is §8) | 0 | 0 | intact | trivially | existing | none |
| **E1** static-copy filler in `RecipeBuf::new` | **−7.8 % / −7.4 %** | 0 (one 480 B static) | intact | identical recipes | existing suite (byte-identical output) | ~5 lines, `ea.rs` |
| **B** lazy memo, cascade fallback | −16.3 % / −14.7 % | 1-2 MiB + ~1 MiB | intact (cascade is the builder) | pure by construction if filled from canonical registers (§5) | partition gate + parity (§7) | `decode()` + a partition |
| **C** eager table, cascade fallback | −13.7 % / −14.7 % | **66.5 MiB** (33 MiB opcode-keyed) | intact | immutable after init | same as B | same as B |
| B/C + E1 | −16.3 % / −16.7 % (C+E1); −15.3…−15.7 % / −13.7…−17.7 % (B+E1) | as B/C | intact | as B/C | as B/C | both |
| **D** generated table source | as C (same content), not run | a ~60 MB Rust literal or a build script; compile-time cost | citations stay on the cascade, which generates it | **can drift** between regenerations; the table and the cascade can disagree silently | needs a staleness gate on top of C's | build system |
| **E2** B + E1 + fact-keyed Bcc/DBcc | **−22.0 % / −22.0 %** | + 134 KiB | intact | pure per (opcode, fact) | + exhaustive fact gate (270 336 files, 0 mismatches) | + two builders take facts |
| **E3** purity from a register-free pass of the cascade (how B/E2 learn the partition) | same hit path as B/E2 | same | intact; no mirror | as B | purity becomes a type error, not a probe verdict | `decode_dispatch` signature, 7 builders |
| **E4** move decode-time resolution into execution | not built | — | recon-pinned recipe shapes change | — | re-pins SST timing | very large: rejected |
| **E5** shrink `MicroState` (MAX_OPS 40, 532 B) | not measured | — | — | — | — | cross-cutting; noted |

D's rejection, re-stated: the seat said it would lose the recon citations. It would not: the cascade stays
the source. It loses because a checked-in table and the live cascade are two artifacts that can disagree,
and a runtime-built C has identical content with no staleness at all.

## 5. Q5: determinism and machine-state hazards

- **Snapshots, `state_hash` and `export_state` cannot see a process-level cache.** `System::snapshot` is
  `bincode::encode_to_vec(self)` over `System`'s fields (`#[derive(Encode)]`); a `static` is not a field.
  `state_hash` reads only the VDP's VRAM, CRAM, VSRAM and registers. `export_state` copies registers, RAM,
  Z80, VDP and SRAM field by field. Cloning a `System` does not clone a static.
- **No aliasing.** A hit returns a *clone* into `Cpu68000.inflight`, and execution mutates that copy
  (`step`, `cycles`, `scratch`, flags). The memo is write-once (`OnceLock`) and never mutated after its
  slot fills. `System::restore` refuses any image with an instruction in flight
  (`check_region(M68kInFlight, Exactly(0))`), so recipes never travel in snapshots at all.
- **Two `System`s in one process share the memo, and cannot observe each other through it.** This is the
  one rule the build must keep: **fill every slot from canonical (or, for facts, witness) registers, never
  from the live registers of the System that happened to touch it first.** Then a slot's content is a
  function of its key alone, and a partition bug yields a wrong-but-deterministic recipe that the gates
  catch, never a history-dependent one. The spike follows this rule. Its example runs 7+ cloned Systems
  sequentially on one warm memo, and every one ends at the same `export_state_hash`. The replay binary,
  the frontend (`refsys`) and `oracle-aether`'s host threads all create several Systems; `OnceLock` gives
  single, thread-safe initialization.
- **The lazy fill is mutable global state, but only in wall-clock terms.** The first touch of a key costs
  one decode plus one `Box` allocation (about 25 ns), once per process. That is timing jitter, not state.
  Memory is bounded by the 65 536 slots.
- **`#![forbid(unsafe_code)]` is met.** `LazyLock`, `OnceLock` and `Box` are safe std. MSRV 1.96 has
  `LazyLock`.

## 6. Q6: the privilege gate and `set_opcode`

- **Keep the gate in front and key on the opcode alone.** Measured: the set of opcodes whose recipe depends
  on S is exactly `is_privileged_opcode`'s 86, with 0 mismatches over all 65 536. Nothing else reads S. The
  memo then holds supervisor recipes (65 536 slots), and user-mode privileged opcodes never reach it.
- **Keep `set_opcode` outside the lookup, in `decode()`, applied to both the hit and the fallback.** It is
  one `u16` store. It keeps the latch at the single site it has today, and makes a memo entry equal to
  `decode_dispatch`'s output.

## 7. Q7: recommendation

**E\*: a lazy per-opcode memo in front of the unchanged cascade, which learns purity from the cascade
itself; plus the static-copy filler; plus fact-keyed entries for Bcc and DBcc.** Measured as E2:
**−22 % on both real playthroughs.** The readable, recon-cited cascade stays the only place a recipe is
defined.

### 7.1 The shape

1. **E1, the static-copy filler** (landing 1, standalone). `RecipeBuf::new` copies its filler from a
   `static [MicroOp; MAX_OPS]` instead of the repeat expression. Byte-identical recipes; −7.5 %; about
   five lines. It also speeds every cascade fallback and every exception recipe.
2. **B with E3, the memo** (landing 2).
   `decode_dispatch(opcode, supervisor, live: Option<&Registers>) -> Dispatch`. The seven
   register-reading builders receive `live`, and on the `None` path return `Dispatch::NeedsRegisters`
   rather than reading anything. `movep_recipe` drops its unused `_regs`. A register-free pass therefore
   *is* the partition: an exact 6 297 per half, where the spike's conservative mirror had 6 873. There is no
   hand-kept list to drift. A new arm cannot read registers without handling `None`, so purity becomes a
   compile-time decision. The memo is 65 536 `OnceLock` slots, keyed by opcode behind the privilege gate,
   and each slot is filled by that pass.
3. **Fact-keyed Bcc and DBcc** (landing 3). Split each into `facts(op, regs) -> Fact` (Bcc: taken;
   DBcc: cond / expired / branch) and `builder(op, Fact)`. The dispatch calls `facts` once, and the memo
   key is `(opcode, Fact)`. The builder can no longer see registers, so the key and the builder read the
   same fact by construction; the spike's `variant_key` was a mirror. That buys a further 6-7 percentage
   points. Scc, TRAPV, the `pos ≥ 16` bit ops and the Dn-count shifts fit the same pattern with 2-64
   facts each, but together they are under 2 % of executed decodes, so their price is unmeasured. MOVEM
   (a 16-bit mask) stays on the cascade.

### 7.2 The gates

A red run is required before each lands, with the mutation shown.

- **G1, partition soundness.** For every opcode the register-free pass memoizes, the directed probe (27
  fields × 3 bases, as in the spike) leaves `decode_dispatch`'s live output unchanged. With E3 this is
  belt-and-braces over a type-level guarantee, and it pins the wiring: the key, the gate's position, S
  handling. **Demonstrated red in the spike:** dropping TRAPV from the partition made the probe report
  2 violations (`0x4e76`, S=0 and S=1, `deps=["V"]`); the baseline reports 0.
- **G2, memo equals cascade over the whole key space.** For all 65 536 opcodes in supervisor mode,
  `memo(op) == cascade(canonical(op))`, and user mode is checked through the gate. **This is the test that
  goes red if the table and the cascade ever disagree.**
- **G3, fact parity.** For every Bcc opcode × all 32 CCR values × S, and every DBcc × 32 CCR × counter
  witnesses {0, 1, 0xFFFF, 0x1_0000, 0xFFFF_0000, 0x8000_0001}: `memo(op, facts(regs)) == cascade(regs)`.
  270 336 register files, 0 mismatches in the spike, 17 ms.
- **G4, live registers across the suite.** The oracle-core release suite (SST corpus, randomized registers
  per case) run with the memo compiled into `decode()`, and the replay playthroughs green. Spike result in
  §9.

### 7.3 What would have to be true for this to be wrong

- **Decode's share is much lower on the workloads that matter.** Both measured workloads are one game
  spinning in VBlank for 55-60 % of its instructions, where decode is proportionally heaviest (TAG-1). If
  the owner's real use is a CPU-bound game, the gain shrinks. It does not reverse: E1 alone costs about
  five lines.
- **Speed is not worth any decode-path complexity in an accuracy-first core.** Then ship landing 1 only
  (E1, −7.5 %, no cache, no partition) and park 2-3.
- **A future builder needs state outside `Registers` at decode time** (bus timing, for example). With E3
  that is a visible signature change, not a silent cache bug; without E3 it would be exactly the silent
  kind.
- **Allocation on first touch is unacceptable** (a real-time audio thread driving the CPU, say). Then use
  the eager opcode-keyed table: same speed within noise, 33 MiB, about 6 ms at startup.

## 8. What this means for M32

`docs/2026-09-11-lens-triage.md` row M32 (104 arms, MOVEQ at arm 101) proposes moving hot arms first.
**Measured: no arm-walk cost is resolvable.** MOVEQ at arm 101 costs the same as MOVE.w at arm 2 within
noise, and the direct-builder control could not separate the walk from zero. Under E\*, a memoized opcode
never walks the cascade after its first touch. The remaining fallback (MOVEM, and until landing 3 the
Bcc/DBcc families at arms 75 and 82) would save only that unresolvable walk. **Close M32 as dissolved by
H22.** A reorder would churn the recon-ordered cascade for no measurable gain.

## 9. Tests run

- **Leg B: oracle-core with the memo compiled into `decode()`.** Ran on `45e14b0` under load 3.1-4.4, with
  `cargo test -p oracle-core --release --features h22-fixed-lazy,h22-fixed-fastfill,h22-fixed-variant`.
  **24 legs, 1 232 passed, 0 failed, 0 ignored (release)**, and no `FAILED` rows. This is G4's evidence:
  every vendored SingleStepTests case (randomized registers per case) and every oracle-core unit test
  decoded through the lazy memo, the static-copy filler and the Bcc/DBcc fact memo, and agreed.
- **Replay playthroughs**: green under every compile-time variant in §2 and §4 (46 timed runs).
- **Leg A: the workspace at the tip.** LEG_A_PLACEHOLDER

## 10. The spike (not proposed for merge)

Feature `h22-spike` (runtime switch) and `h22-fixed-{fastfill,table,lazy,double,variant}` (compile-time
variants) in `crates/oracle-core/Cargo.toml`; all default off. Files: `src/m68000/h22_spike.rs` (a child
module of `decode`, so it can reach the private dispatch), `examples/h22_decode_spike.rs`, and cfg-gated
hooks in `decode()` and `RecipeBuf::new`. Commits:

- `6743ea7` harness, probe, table, first real-ROM A/B;
- `8c58e65` materialization/walk split, static-copy filler;
- `9d2d02d` lazy memo, compile-time variants, the honest A/B;
- `45e14b0` fact-keyed Bcc/DBcc memo and its exhaustive gate; the red demo of the partition gate is recorded
  in its body.

The branch's following commit (`spike: remove the H22 harness from the tip`) returns every touched file to
`d77182c`'s content, so **a whole-branch merge lands only this document**. To reproduce, check out `45e14b0`.

Reproduce:
- the probe and the variant gate: `cargo run --release --features h22-spike --example h22_decode_spike --
  --probe-only`;
- the full harness: the same command without `--probe-only`;
- a timed variant: `cargo test -p oracle-replay --release --features oracle-core/h22-fixed-lazy,oracle-core/h22-fixed-fastfill,oracle-core/h22-fixed-variant --test replay_real_artifacts the_standing_fixture_runs_green -- --exact`,
  against the same command with no `--features`.

## 11. Open items and TAGs

- **TAG-1 (workload breadth).** Every number comes from Sonic 4 on aeon (the frozen `s4.debug.bin` and the
  two OJZ replay fixtures), all of which spin in VBlank. Re-measuring the add-one decode share on a
  CPU-bound commercial ROM would bound the gain from below. This needs a ROM this repo can run
  headlessly; I did not pick one.
- **TAG-2 (the GUI path).** The player and frontend drive the core through their own loops; the
  wall-clock gain there was not measured, and no emulator tools were used, by the parcel's rules. The
  headless core gain is what is measured.
- The replay test file's release-time table (about 4 s and 6 s) is stale (§0.7). Not touched here.
- Nothing was BLOCKED.
