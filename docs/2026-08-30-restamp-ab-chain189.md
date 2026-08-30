# Restamp A/B — chain-189 candidate vs. our frozen chain-186 control

**Date:** 2026-08-30 · **Branch:** `parcel/restamp-ab` (worktree off `main` at `05859b3`)
**Nature of this run:** a MEASUREMENT for the aeon lane. Dry run throughout. Nothing was written,
patched, or re-stamped — no `--out`, no `--allow-source-write`, no `--force`, no `--fixture-bin`.

---

## Headline, in the order it must be read

1. **The control moved and the candidate did not.** This is the reverse of the arrangement the A/B
   was set up to test, so the control result leads.
   - Control (chain-186) / `ojz_fixture`: **9 of 27 stale**, indices **18–26**.
   - Control (chain-186) / `ojz_slide_fixture`: **0 of 37**, clean.
   - Candidate (chain-189) / `ojz_fixture`: **0 of 27**, clean.
   - Candidate (chain-189) / `ojz_slide_fixture`: **0 of 37**, clean.
2. **The control's 9 are NOT manufactured by the restamp instrument.** A plain, uninstrumented
   run of the control ROM (no `--restamp`, no recovery stub) DESYNCs at `Logic_Tick 1154`,
   `expected $A6CC0AEB actual $607BF6C4` — exactly row 18 of the restamp table, the first stale
   row. The instrumented pass and the plain run agree. See "Was it the instrument?" below.
3. **Our own committed control pin fails its own green test today.**
   `the_standing_fixture_runs_green` (`#[ignore]`d, so it does not run in the default suite) FAILS
   against `fixtures/aeon/`. `the_slide_fixture_runs_green` passes. Registered here as a finding
   about our repo's pin; not repaired by this run.
4. **The candidate produced no moved set at all.** There is nothing to hand aeon as a restamp
   plan for chain-189. A null result, reported as-is.

---

## Revisions and artifacts

| | value |
|---|---|
| candidate build | sigil `39c34fd2`, chain 189 |
| control build | sigil `5af70797` (chain 186), `aeon_rev def98ee5` (per `fixtures/aeon/PROVENANCE.md`, committed `090784a`) |
| aeon rev of record for this request | `3f143178` |

| artifact | sha256 | bytes |
|---|---|---|
| candidate `restamp-ab-chain189/s4.debug.bin` | `4ee7ac79737f1decc16c13cef4e160ed26c3fea078b3f5b2b7c4300857a9a0b3` | 736315 |
| candidate `restamp-ab-chain189/s4.debug.lst` | `81a111020e3f28ddda374648e7a3e1425cbde00ce5b09ea5769b83eb79845a2f` | — |
| control `fixtures/aeon/s4.debug.bin` | `75e9f4d4b7fb8ab0f9880b43d20622abef4ef1e4b672694ae6921f71619fcf7a` | 736095 |
| control `fixtures/aeon/s4.debug.lst` | `d478dec2c7a771d0485a6dab9b52c48a1f63323439da10ab018af7d64d4feccb` | — |

**Candidate provenance re-derived, not copied.** The authority is sigil's committed blob:

```sh
cd /home/volence/sonic_hacks/sigil
git cat-file -p 39c34fd2:crates/sigil-harness/golden/s4.debug.bin | sha256sum
# 4ee7ac79737f1decc16c13cef4e160ed26c3fea078b3f5b2b7c4300857a9a0b3
```

This equals the snapshot's hash. The snapshot is the committed blob.

---

## THE MOVED SETS, verbatim and complete

`expected` is the value the fixture carries (the stale one); `actual` is what the machine produced.
That is the direction `restamp::patch4` uses: `patch4(rom, payload, expect_old = expected, new = actual)`.

### Control (chain-186) — `ojz_fixture` — 9 of 27 stale

```
   idx   ring   tick     rom_off   fix_off   expected    actual
    18   1152   1154    0A6CDC    0000AC    A6CC0AEB    607BF6C4
    19   1216   1218    0A6CE6    0000B6    C3374606    3750A813
    20   1280   1282    0A6CF6    0000C6    BC3A6AE9    AF65AF6A
    21   1344   1346    0A6CFE    0000CE    157A30D9    1F8422C5
    22   1408   1410    0A6D08    0000D8    2DAD7B9A    93161A26
    23   1472   1474    0A6D14    0000E4    E7AA42D7    F3185259
    24   1536   1538    0A6D1E    0000EE    855704EA    5B1CF76D
    25   1600   1602    0A6D26    0000F6    F490D9DD    5B22F780
    26   1664   1666    0A6D30    000100    B55CAD46    2FA77A39
```

Stale indices: **18, 19, 20, 21, 22, 23, 24, 25, 26 — of 27 total (0–26).** Every checkpoint from
18 to the end of the stream inclusive; checkpoints 0–17 all matched.

### Control (chain-186) — `ojz_slide_fixture` — 0 of 37 stale

`NOTHING TO RE-STAMP — every checkpoint matched.` The moved set is empty.

### Candidate (chain-189) — `ojz_fixture` — 0 of 27 stale

`NOTHING TO RE-STAMP — every checkpoint matched.` The moved set is empty.

### Candidate (chain-189) — `ojz_slide_fixture` — 0 of 37 stale

`NOTHING TO RE-STAMP — every checkpoint matched.` The moved set is empty.

---

## Precision caveat — a bound, not a location

Checkpoints in these streams fire every **64 ticks** (visible in the `ring`/`tick` columns above:
1152/1154, 1216/1218, …). So a row reading "checkpoint 18 / tick 1154" **bounds** the divergence to
the window **ticks 1091–1154**. It does not locate it. The divergence may have entered anywhere in
that 64-tick window; checkpoint 18 is only the first place the stream was able to observe it.

The same caveat applies to the shape of the set. "Indices 18–26" is the set of checkpoints whose
recorded hash disagrees. Once a run diverges, every downstream checkpoint disagrees as a
consequence, so a contiguous tail from 18 onward is what a *single* divergence bounded to
ticks 1091–1154 would produce — the count 9 is not 9 independent events.

---

## Was it the instrument? — the control's 9, tested directly

The brief requires that a control reporting movers outranks the candidate result. It reported 9, so
it was tested rather than assumed.

**Determinism.** The control `ojz_fixture` restamp pass was run twice. The two outputs are
byte-identical modulo the two wall-clock timing lines (`one pass, N s` / `PASS in N s`). Not a
nondeterminism artifact.

**Instrument perturbation.** The restamp pass installs a recovery stub over `Input_Tick.desync`
so a stale checkpoint stops rather than traps. That stub is the one thing that could manufacture
movers. It was taken out of the loop entirely:

```sh
./target/release/replay_runner --rom fixtures/aeon/s4.debug.bin \
  --lst fixtures/aeon/s4.debug.lst --fixture ojz_fixture      # NO --restamp, NO stub
# EXIT=2 (DESYNC)
DESYNC — a checkpoint did not match.
  Logic_Tick 1154   expected $A6CC0AEB   actual $607BF6C4
  raised at $002718  (Input_Tick.desync+$4)
```

The plain run's desync is row 18's tick, row 18's `expected`, and row 18's `actual`, to the bit. The
instrumented pass and the uninstrumented pass agree on the first divergence.

**Corroborated a third way** by the tool's own self-verification, which is a *plain* run: applying
the 9-row plan to a pristine copy of the control ROM produced an image that ran clean end-to-end
(`Logic_Tick 1723`, all 27 compared and matched) and on which the negative control still tripped.
The re-stamped values are therefore the values a plain run produces.

**Conclusion on the instrument:** the 9 movers are genuine staleness in the control artifact
`fixtures/aeon/s4.debug.bin` itself, not an artifact of the restamp method. The method did not
manufacture them. What the control does *not* do in this A/B is serve as a clean baseline — it is
itself stale on `ojz_fixture`.

---

## Secondary finding — the committed pin fails its own green test

`crates/oracle-replay/tests/replay_real_artifacts.rs` carries `the_standing_fixture_runs_green`,
which asserts `Verdict::Pass` for `ojz_fixture` against `fixtures/aeon/`. It is `#[ignore]`d
(annotated "full playthrough: ~34 s unoptimized"), so it does not run in the default suite.

```sh
cargo test --release -p oracle-replay --test replay_real_artifacts -- --ignored \
  the_standing_fixture_runs_green the_slide_fixture_runs_green
```

```
test the_standing_fixture_runs_green ... FAILED
test the_slide_fixture_runs_green ... ok
...
ojz_fixture must pass, got Trap(... desync: Some(DesyncDetail {
    actual: 1618736836, logic_tick: 1154, expected: 2798389995 }) ...)
test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 14 filtered out; finished in 13.36s
```

`1618736836 = $607BF6C4`, `2798389995 = $A6CC0AEB` — the same row 18. The pin was committed in
`090784a` ("freeze our own copy of Aeon's artifacts: six committed blobs, chain 186"). Whether the
ignored playthrough was run at freeze time is not established by this run. Left open; not repaired
here.

---

## Exact commands run

Build (release; the debug path is roughly an order of magnitude slower here):

```sh
cargo build --release -p oracle-replay --bin replay_runner
```

The four measurement passes — dry run, no output flags of any kind:

```sh
# control (chain-186), both fixtures
./target/release/replay_runner --rom fixtures/aeon/s4.debug.bin \
  --lst fixtures/aeon/s4.debug.lst --fixture ojz_fixture       --restamp
./target/release/replay_runner --rom fixtures/aeon/s4.debug.bin \
  --lst fixtures/aeon/s4.debug.lst --fixture ojz_slide_fixture --restamp

# candidate (chain-189), both fixtures
./target/release/replay_runner --rom /home/volence/sonic_hacks/restamp-ab-chain189/s4.debug.bin \
  --lst /home/volence/sonic_hacks/restamp-ab-chain189/s4.debug.lst --fixture ojz_fixture       --restamp
./target/release/replay_runner --rom /home/volence/sonic_hacks/restamp-ab-chain189/s4.debug.bin \
  --lst /home/volence/sonic_hacks/restamp-ab-chain189/s4.debug.lst --fixture ojz_slide_fixture --restamp
```

The four plain (non-restamp) confirmation runs, same two ROMs and fixtures with `--restamp` dropped:

| run | exit |
|---|---|
| control / `ojz_fixture` | **2 (DESYNC)** at `Logic_Tick 1154` |
| control / `ojz_slide_fixture` | 0 (PASS) |
| candidate / `ojz_fixture` | 0 (PASS) |
| candidate / `ojz_slide_fixture` | 0 (PASS) |

Every `--restamp` run reported `output   DRY RUN — no --out, so nothing will be written anywhere`.
The control runs additionally reported the repo write guard engaging
(`guard    …/agent-a016a8c9895321251 is protected (the inputs came from it)`), since the control
inputs live inside this repository.

## Totals and wall clock

| pass | in-tool pass time | wall clock |
|---|---|---|
| `cargo build --release` | — | 2.74 s |
| control / `ojz_fixture` `--restamp` | 4.31 s pass + 4.17 s verify | 8.56 s (09:14:48Z → 09:14:57Z) |
| control / `ojz_slide_fixture` `--restamp` | 5.72 s | 13 s (09:15:10Z → 09:15:23Z) |
| candidate / `ojz_fixture` `--restamp` | 4.24 s | 13 s (09:15:29Z → 09:15:42Z) |
| candidate / `ojz_slide_fixture` `--restamp` | 5.73 s | 14 s (09:15:49Z → 09:16:03Z) |
| the two `#[ignore]`d green tests | — | 13.36 s |

Both ROMs armed at frame 34. Both `ojz_fixture` runs reached `Logic_Tick 1723` over a declared 1721
ticks; both `ojz_slide_fixture` runs reached 2352 over 2350. Both listings bound to their image
(control 2732 symbols, candidate 2743).

---

## Open, and why

- **No moved set exists for the candidate.** Nothing is handed to aeon as a restamp plan for
  chain-189; the candidate ROM's embedded fixtures already agree with what the machine produces on
  both streams.
- **The control pin's staleness is not diagnosed here.** This run establishes that it is real and
  reproducible without the instrument, and bounds its onset to ticks 1091–1154 of `ojz_fixture`. It
  does not establish what changed between the pin and the fixture recorded inside it, nor whether
  the pin or the fixture is the side that should move. That is a call for whoever owns the pin.
- **No runtime confirmation was attempted.** Emulator MCP tools are off-limits from background
  agents in this workspace; anything wanting live confirmation is tagged for foreground follow-up.

---

## Overseer verification, and the finding the measurement pass did not have

Everything above was **reproduced firsthand** at `6bab943` before any of it was sent across the
fence: the control's 9 rows byte-for-byte with identical payloads, the candidate clean on both
fixtures (`0 of 27`, `0 of 37`), and `the_standing_fixture_runs_green` failing today with
`expected 2798389995` / `actual 1618736836` at `logic_tick 1154` — decimal for `$A6CC0AEB` /
`$607BF6C4`, which is row 18 to the bit.

### ⚑ The candidate is clean BECAUSE it already carries the repair — and the repair is ours

A byte search of the chain-189 image settles why it moved nothing:

| searched for | in chain-189 ROM |
|---|---|
| the control's 9 **stale** payloads (`A6CC0AEB` …) | **ABSENT, all nine** |
| the control's 9 **actual** payloads (`607BF6C4` …) | **present, all nine, at `$0A6CDC…$0A6D30`** |

Those are the **same offsets** our control pass names, and the values are exactly the repair our
runner computed from the stale side. So chain 189's fixture **has already been re-recorded**, and
what it was re-recorded to is byte-identical to what this instrument derives independently.

**Two consequences, and they pull in opposite directions.**

1. **The A/B as designed could not be taken, and aeon's falsifier was not tested.** The booking
   specified *"the ROM with the new clamps and the OLD fixture"*, because the measurement is what
   decides what to re-record. Chain 189 is new clamps and a **new** fixture, so there is no moved
   set in it, and **nothing here bears on their early-checkpoint prediction either way.** A clean
   result is not a confirmation. Reporting `0 of 27` as agreement with their mechanism would be
   precisely the confirming summary the booking forbids.
2. **We got a stronger check than the one we planned.** Two instruments, two ROMs, no contact:
   our runner reading chain 186 derives the nine payloads aeon's chain-189 freeze already contains.
   That is independent corroboration that **the re-record they shipped is correct**, obtained from
   the stale side rather than by agreeing with them.

**And a third thing follows that neither lane asked for.** Chain 186's machine and chain 189's
machine produce the **same** checkpoint hashes at 18–26 — 189's fixture holds what 186's ROM
produces, and 189's ROM matches its own fixture. So **the clamps changed nothing observable at
these nine checkpoints.** Stated as the bound it is, not as a general claim about the clamps.

### What the stale set is, and what it is not

The set is `{18–26}` — **exactly** the set aeon attributed to `fde35b2f` at chain 181. Our freeze
at chain 186 inherited it and still carries it. This is [[REPLAY-NET-BLIND-3]] arriving as a
measurement rather than a prediction: the pin is stale, and the one test that would say so is
`#[ignore]`d, so the default suite is green over it.

⚠ **Do not read this as evidence against aeon's clamp mechanism.** The divergence sits at the
**late** tail (ticks 1091–1666), the opposite end from their `cam_col < 16` prediction — but it is
**not clamp-derived**: they dated it to `fde35b2f`, five chains earlier. Same fixture, different
divergence. Answering their question with this data would be a category error.

**Precision, kept:** checkpoints fire every 64 ticks, so *"checkpoint 18 / tick 1154"* **bounds**
the onset to ticks 1091–1154 rather than locating it, and the contiguous 18-to-end tail is what one
divergence in that window produces once downstream hashes inherit it. **The count 9 is not nine
independent events**, and must not be reported as nine.

---

## Postscript — aeon's reply, and the correction that goes back the other way

aeon corrected one thing here and I accept it: the hybrid I said would need building **already exists**
as chain 188 (sigil `e38295d2`, `951cf960…`, 736315 B), so the right statement was *"nothing NEW bears
on the prediction"* rather than *"there is no set here to judge you by."* Mine was over-stated.

**Their 40-byte 188→189 accounting is exact — verified here byte-wise, 12 runs, 40 bytes:**
`$00018E-8F` checksum word (2) · `$0A6C46` `1D`→`0D` standing checkpoint 0 (1) · nine 4-byte payloads
at `$0A6CDC…$0A6D33`, checkpoints 18–26 (36) · `$0A6D56` `1D`→`0D` slide checkpoint 0 (1). No code
bytes move, so **chain 189 is a pure fixture re-record**, corroborated from the byte side.

### ⚑ But the moved set they recorded for chain 188 is INCOMPLETE, and it is the falsifier that pays

They reported the 188 A/B as returning moved set **`{0}`**. Measured firsthand here — chain-188 ROM,
chain-189 listing, a pairing that is **sound because the two ROMs differ in zero code bytes**, so the
symbol table is identical by construction (stated openly so it can be objected to):

**`--restamp` on chain 188 returns `10 of 27`: `{0, 18, 19, 20, 21, 22, 23, 24, 25, 26}`.**

Checkpoint 0 is `1D375066 → 0D375066` at tick 2; 18–26 are the same nine payloads as our chain-186
control. **Those ten rows ARE the 188→189 delta**, independently measured two ways, which closes the
loop: the re-record chain 189 ships is exactly this restamp plan.

**What that does to the falsifier.** aeon's stated test was: *"If checkpoints deep into the run also
moved, my mechanism is incomplete and the restamp must not proceed on it."* Checkpoints 18–26 sit at
ticks 1091–1666 — **deep**. So the falsifier **fired**. It survives only under the refinement that
18–26 have a separately-dated cause (`fde35b2f`, chain 181, 17.8 hours before the clamps), which their
own byte-identity archaeology established and which this lane independently corroborates.

That refinement is legitimate and the restamp of all ten was correct. **The record should nonetheless
say "the falsifier fired, and the firing was explained by an independently-dated cause" rather than
"the falsifier was applied and survived."** Those are different epistemic states, and the difference is
load-bearing precisely because their own rule made deep movers a stop condition. A prediction that
holds after an exception is carved for the thing that broke it has not been tested the way an
untouched prediction has.

### Ops — a shell trap that hashes nothing and calls it a result

Reproduced firsthand, and it is worth the line because this lane reads across repos constantly. In zsh,
`git cat-file -p $rev:crates/…` applies the **`:c` history modifier** to the parameter: the pathspec
becomes `39c34fd2rates/…`, git fails to **stderr**, and a pipeline into `sha256sum` hashes **empty
input** and returns `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` — the sha256 of
nothing — with the failure masked by the pipe. **Brace it: `"${rev}:crates/…"`.**

⚠ It bites only through a **variable**; a literal `39c34fd2:crates/…` is unaffected, because the
modifier needs a parameter expansion to attach to. Every hash in this document came from the literal
form, so nothing here is contaminated — checked rather than assumed. Credit: aeon, who hit it twice and
caught it only because `e3b0c442` was familiar. **That is the tell to memorise**, since the failure
presents as a plausible hash rather than as an error.

---

## ⚑ RETRACTION — "the falsifier fired" is WRONG, and the disproof is this document's own earlier measurement

**Retracted in full:** the section above claiming aeon's falsifier fired and survives only via a
post-hoc exception. Both halves of that are wrong. Left standing rather than deleted, because the
error is better evidence for the lesson than the argument was.

**The measurement.** Chain 186 and chain 188 compared at all eleven checkpoint sites — verified here,
not accepted from the reply:

```
$0A6CDC $0A6CE6 $0A6CF6 $0A6CFE $0A6D08 $0A6D14 $0A6D1E $0A6D26 $0A6D30   all SAME
$0A6C46  186=1D  188=1D  SAME          $0A6D56  186=1D  188=1D  SAME
11 of 11 identical
```

So `stale(188) = {0, 18–26}` and `stale(186) = {18–26}`, and **the falsifier is stated over MOVED, not
STALE**: `moved = stale(188) \ stale(186) = {0}` — checkpoint 0, `cam_col` 6, **inside** the window.
**The prediction held. Nothing deep moved.**

**I had already measured this and then argued against it.** Two sections up, in the postscript's own
words: *"186's machine and 189's machine produce the same hashes at 18–26, so the clamps changed nothing
observable at these nine checkpoints."* A checkpoint the clamps changed nothing at **did not move**.
The ten I found is chain 188's **stale** set, which aeon's entry also records as ten; the differential
is one.

**And the methodological charge does not hold either.** The rule — a prediction surviving because an
exception was carved for the thing that broke it has not been tested — is sound in general and does not
apply here, **for a reason that is a date rather than a preference**. Candidate-versus-control was the
**booked design**, with chain 186 named as control *before the run*. What excludes the nine is
byte-identical actual-hashes on both ROMs, available the moment the A/B returned. The `fde35b2f` dating
came later and explains *why* they were stale; it is **not what excludes them**. A carved exception
would require the control to have been chosen, or the set narrowed, after seeing which checkpoints
misbehaved. Neither happened.

### The lesson, which is the part worth keeping

**STALE and MOVED are different sets, and the falsifier turns on the word.** aeon's entry used "movers"
in two senses one sentence apart; that ambiguity was real, it is what made my reading available, and
they have separated the terms and restated the falsifier over MOVED. That catch stands.

**But I then made the same conflation from the other side, holding my own correct measurement.** Their
entry already warned that an *uncontrolled* reading of chain 188 meets the falsifier word for word, and
named that as the exercise's lesson: **a falsifier is only as good as the control behind the measurement
it is applied to.** This lane independently re-derived the uncontrolled reading and reached the
predicted wrong conclusion — the documented trap, sprung on a second lane doing careful work, in writing.

⚠ **The specific failure mode, for the next time:** I ran the right measurement, wrote the right bound
(*"the clamps changed nothing observable at these nine"*), and then reasoned about a **raw count** from
a later run instead of the **differential** I already held. The control existed, was booked, and had
been measured — and I dropped it out of the comparison at the moment the numbers got interesting.
**A differential is not a count, and holding the control is not the same as using it.**
