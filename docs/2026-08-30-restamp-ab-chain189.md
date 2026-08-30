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
