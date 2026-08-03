# Decision 2 (T12 Mode-4 register masking) — premise re-check by measurement

**Date:** 2026-08-03  **Branch:** `m68000-microop-framework`  **Tree HEAD at measurement:** `5a0f69d`
**Method:** three throwaway copies of the tree in a scratch directory. The working tree was never
modified; nothing but this document is committed. Clean-room respected — no emulator source outside
this repo was opened.

---

## Verdict up front

| Parked claim | Verdict |
|---|---|
| 1. `export_state_v1::GOLDEN_HASH` moves | **FALSE.** It does not move. Same mis-identified fixture as Decision 1. Measured: with the fix applied, `export_state_hash` is still `0xBF5D_1E1A_A727_143B`, and all of `export_state_v1` passes. |
| 2. `golden_frames` scene hashes move | **HALF TRUE, and the important half is FALSE.** 6 of 7 `golden_frames` tests *do* go red if the fix lands untouched — but **not one pinned hash constant needs regenerating.** Declaring mode 5 in the fixture (one `control_write` + a `$40`→`$44`) restores all six frozen hashes byte-identically. |
| 3. "52 unit tests" go red | **TRUE — the number is now 53** (52 + the `bus::tests::vdpfifo_t4_fill_trigger_and_byte_placement` test that A3b added in `0514625`, which the doc predates). Plus **9 more** in integration tests the parked doc did not list. All 62 are one bug: the fixture never sets M5. Zero indicate a behavioral regression; all 62 go green under a test-only, 8-site mechanical repair. |
| 4. Scorecard: what does T12 alone buy | **`vdp_port_access` 13/3/16 → 14/2/16** (page 1 unchanged at 8/1/9). Exactly as forecast. It is the **only** conformance row that moves. |

**Recommendation: Decision 2 no longer needs an owner ruling.** No frozen currency moves. It is a
routine currency-neutral slice — a one-line VDP fix plus a mechanical, test-only correction of
fixtures that were configuring an impossible machine (Mode 4 while programming Mode-5 registers),
plus the usual non-gating `BASELINE` scoreboard bump that every conformance win produces.

The parked doc's framing — *"the golden fixture drives configs real hardware ignores, so
`GOLDEN_HASH` encodes our bugs"* — is now **0 for 2**. Both decisions traced to
`testrom::build_pad_poll`, which is not a currency fixture at all.

---

## Setup (identical for all three scratch trees)

```bash
S=<scratchpad>
rsync -a --exclude target --exclude vendor --exclude .git \
      /home/volence/sonic_hacks/oracle-next/ "$S/scratch-tree/"
ln -sfn /home/volence/sonic_hacks/oracle-next/vendor "$S/scratch-tree/vendor"   # or rows silently SKIP
```

* `scratch-tree` — pristine baseline.
* `scratch-patched` — baseline + the **one-line fix only**.
* `scratch-fixed` — `scratch-patched` + a **test-only** fixture repair.

The fix, inserted in `Vdp::write_register` (`crates/oracle-core/src/vdp.rs:610`) directly after the
`reg >= REG_COUNT` guard and before the M3/HV-latch logic (reg 0 is ≤ 10, so that path is untouched):

```rust
if self.regs[1] & 0x04 == 0 && reg > 10 {
    return;
}
```

**Baseline, for reference** (`cargo test -p oracle-core` in `scratch-tree`, full run, exit 0):
lib **698 passed / 0 failed / 1 ignored**; every integration target green.

---

## Claim 1 — `export_state_v1::GOLDEN_HASH`: DOES NOT MOVE

### Which fixture is which

* `crates/oracle-core/tests/export_state_v1.rs:52` → `oracle_core::testrom::build()`.
* `testrom::build()` (`crates/oracle-core/src/testrom.rs:77-131`) sets reset/exception vectors and a
  RAM-stirring loop at `$200`. **It contains no VDP port address and writes no VDP register.**
* The ROM that writes `reg 1 = $8150` is `testrom::build_pad_poll()`
  (`crates/oracle-core/src/testrom.rs:190+`). Its consumers are `tests/io_controllers.rs`,
  `tests/watchpoints.rs`, and `examples/pad_probe.rs` — **no frozen currency**.

This is precisely the mis-identification that sank Decision 1's premise, repeated verbatim in
Decision 2 and in the `NOT MODELLED` comment at `crates/oracle-core/src/vdp.rs:619`.

### Measurement (not inspection)

In `scratch-patched` (fix applied), a throwaway probe was appended to `tests/export_state_v1.rs`:

```rust
#[test]
fn scratch_probe_golden_fixture_touches_no_vdp_registers() {
    let sys = fixture();
    let regs = sys.vdp().regs();
    println!("SCRATCH-PROBE vdp regs after 60 frames = {regs:02X?}");
    assert!(regs.iter().all(|&r| r == 0), "golden fixture writes no VDP register");
    println!("SCRATCH-PROBE export_state_hash = {:#018X}", sys.export_state_hash());
}
```

```
$ cargo test -p oracle-core --test export_state_v1 -- --nocapture
SCRATCH-PROBE vdp regs after 60 frames = [00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00,
                                          00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00]
SCRATCH-PROBE export_state_hash = 0xBF5D1E1AA727143B
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

`0xBF5D1E1AA727143B` is the pinned `GOLDEN_HASH` unchanged. All 24 VDP registers are `$00` after 60
frames, so `write_register` is never reached with this fixture and the mask is a structural no-op
for it. `Vdp::power_on` initialises `regs: [0u8; REG_COUNT]` deterministically
(`crates/oracle-core/src/vdp.rs:223`), so this is seed-independent — worth stating, because if
registers had been RNG-seeded, the mask's effect would have depended on `SEED`.

**Adversarial check that the patch was actually live in that build:** the same binary tree took the
lib suite from 698/0 to 645/53 and turned `golden_frames` red. The fix was unquestionably active;
`export_state_v1` simply cannot see it.

---

## Claim 2 — `golden_frames`: the tests go red, the pinned hashes do not move

### Fix only (`scratch-patched`)

```
$ cargo test -p oracle-core --test golden_frames
test result: FAILED. 1 passed; 6 failed
    golden_frame_scene_1_priority_shadow_highlight
    golden_frame_scene_2_r8_partial_column
    golden_frame_scene_3_r9_window_bug
    golden_frame_scene_4_hscroll_modes
    golden_frame_scene_5_r5_cache_window
    golden_frame_scene_6_no_mid_sprite_cut
```

Cause: every scene does `set_reg(&mut v, 0x01, 0x40)` — display enable, **M5 clear** — and then
programs registers `$0B`, `$0C`, `$0D`, `$0F`, `$10` (`crates/oracle-core/tests/golden_frames.rs:121-129`
and the equivalent block in each scene). With the mask live, the autoincrement and plane/window
registers are dropped and the scenes degenerate — scenes 5 and 6 even collapse to the *same* hash
(`9321244264401117989`), which is the signature of a fixture that stopped configuring anything, not
of a rendering change.

*(A grep for `set_reg(v, 11,` finds nothing because the fixtures write register numbers in hex —
this is why the parked doc's spot-check is easy to get wrong in either direction.)*

### Fix + mode-5 declaration (`scratch-fixed`)

Test-only edit, 7 lines:

```rust
fn fresh() -> Vdp {
    let mut v = Vdp::power_on(&mut SplitMix64::new(1));
    v.vram_mut().fill(0);
    v.control_write(0x8104, 0); // reg 1 = $04 → M5 set (mode 5)
    v
}
```
plus `set_reg(&mut v, 0x01, 0x40)` → `0x44` in all six scenes.

```
$ cargo test -p oracle-core --test golden_frames
test result: ok. 7 passed; 0 failed
```

**All six pinned scene-hash constants are still correct, byte-identical.** No regeneration, no
ledger entry, no "deliberate, evidenced amendment" of the kind `golden_frames.rs`'s header demands.
That is the substantive refutation: the parked doc treated "the test goes red" and "the currency
moves" as the same event. Here they are not.

---

## Claim 3 — the red-test count: **53** lib tests (+9 integration), all one fixture defect

### Fix only (`scratch-patched`)

```
$ cargo test -p oracle-core --lib
test result: FAILED. 645 passed; 53 failed; 1 ignored
```

Reconciles exactly with the parked doc's 52: the extra one is
`bus::tests::vdpfifo_t4_fill_trigger_and_byte_placement`, introduced by A3b in commit `0514625`
(`git log -S` confirms), after the doc was written.

By module:

| Module | Red | Fixture defect |
|---|---|---|
| `render::tests` | 45 | `fresh()` = bare `power_on` (regs all `$00`); scenes write reg 1 = `$40`/`$00`, then program `$0C` (H40), `$11` (window), `$0B`/`$0D`/`$0F`/`$10` |
| `bus::tests` | 4 | `MdMem::new`'s `Vdp::power_on`; tests then use autoinc (`$0F`), DMA regs (`$13`/`$14`/`$17`) |
| `z80::bus::tests` | 2 | `fresh_vdp()` = bare `power_on`; discriminator writes `$8F02`/`$8F44` |
| `vdp::tests` | 1 | `register_write_never_arms_the_toggle` writes reg 15 from a bare `fresh()` |
| `system::tests` | 1 | `scanline_wiring_…` writes reg 15 then reg 5 on a booted system |

Integration, not listed in the parked doc:

| Target | Red | Fixture |
|---|---|---|
| `golden_frames` | 6 | see Claim 2 |
| `watchpoints` | 2 | `testrom::build_pad_poll` (reg 1 = `$8150`); its DMA-fill and CRAM writes need regs 19/20/23 and 15 |
| `io_controllers` | 1 | same ROM |

Green throughout, both with and without the fix: `determinism_gate`, `scanline_capture`,
`oracle_differential`, `proptests`, `export_state_v1`.

### Is any of it a real behavioral regression? No — argued two ways.

**Structurally.** The patch's *only* effect is to drop a register write when `regs[1] & 0x04 == 0`.
There is no other mechanism by which any observable can change. Every difference must therefore
trace to a dropped register write.

**Empirically.** An 8-site, test-only repair takes all 62 back to green with **every frozen constant
untouched**:

1. `render.rs` `fresh()` + `control_write(0x8104)`; `set_reg(…,0x01,0x40)`→`0x44` ×2, `0x00`→`0x04` ×2 → fixes all 45
2. `golden_frames.rs` `fresh()` + `control_write(0x8104)`; `0x40`→`0x44` ×6 → fixes all 6
3. `bus.rs` `MdMem::new`'s vdp field + `control_write(0x8104)` → fixes all 4
4. `vdp.rs` `fresh()` + `control_write(0x8104)`; `regs_with_reg15()` gains `r[0x01] = 0x04` → fixes 1
5. `z80/bus.rs` `fresh_vdp()` + `control_write(0x8104)` → fixes 2
6. `system.rs` scanline test + `control_write(0x8104)` before the reg-15 write → fixes 1
7. `testrom.rs` `build_pad_poll` reg 1 `$8150` → `$8154` → fixes 3 (io_controllers 1, watchpoints 2)

```
scratch-fixed $ cargo test -p oracle-core --lib
test result: ok. 698 passed; 0 failed; 1 ignored
scratch-fixed $ for t in export_state_v1 golden_frames determinism_gate scanline_capture \
                         io_controllers watchpoints oracle_differential proptests; do
                   cargo test -p oracle-core --test $t; done
export_state_v1      ok. 4 passed; 0 failed        golden_frames        ok. 7 passed; 0 failed
determinism_gate     ok. 2 passed; 0 failed        scanline_capture     ok. 3 passed; 0 failed
io_controllers       ok. 2 passed; 0 failed        watchpoints          ok. 8 passed; 0 failed
oracle_differential  ok. 3 passed; 0 failed        proptests            ok. 3 passed; 0 failed
```

`testrom::build_vram_poke` needed **no** change: its only reg-11+ write is autoinc, and a word write
to the data port stores both bytes regardless — so its two watchpoint tests were green already.

And the spec test the arc parked:

```
$ cargo test -p oracle-core --lib -- --ignored
test vdp::tests::mode4_ignores_register_writes_above_ten ... ok
```

### Are the fixtures testing an impossible machine state?

**Yes, honestly.** Every one of the 62 intends Mode 5 — they program H40, the window, plane bases,
autoincrement, DMA — while leaving reg 1 bit 2 clear, which on hardware selects Mode 4. The parked
doc's read that this is *"a signal — a lot of our fixtures quietly configure a machine that could not
exist"* is correct and is the one part of Decision 2 that survives intact. It is worth fixing on its
own merits. It is also, measurably, about ten minutes of mechanical work rather than "real work
budgeted as its own slice."

---

## Claim 4 — scorecard

`scratch-patched` and `scratch-fixed` agree, and only one row moves:

```
patched : ("vdp_port_access", "page1 pass/fail/total=8/1/9; pages1+2 cumulative=14/2/16")
BASELINE: ("vdp_port_access", "page1 pass/fail/total=8/1/9; pages1+2 cumulative=13/3/16")
```

All 16 other `BASELINE` rows are byte-identical (diffed programmatically over the failure payload).
So **T12 alone: 13/3/16 → 14/2/16**, page 1 unchanged at 8/1/9 (T12 is a page-2 test). This matches
the parked doc's arithmetic once its stale pre-A3b cumulative is corrected. `BASELINE` is the
explicitly **non-gating** scoreboard, updated with every conformance win (A3b did exactly this) —
it is not frozen currency.

The remaining `vdp_port_access` tail after T12 is T6 (slice A4) and T16 (per-line access-slot
scheduling).

---

## Coverage and caveats (adversarial notes on my own measurements)

* **SST suites are structurally immune.** `tests/singlestep_m68000.rs` uses `FlatBus`;
  `tests/singlestep_z80.rs` uses a `Z80Io` stub. Neither constructs a `Vdp`, so `write_register` is
  never called. The pristine baseline full run (`cargo test -p oracle-core`, exit 0) covers them
  unpatched; a patched re-run was also started for completeness.
* **No `tail`/`head` on any `cargo test`** — every run was redirected to a log and grepped, per the
  standing rule.
* **`vendor` was symlinked** into each scratch tree; the conformance run reported real per-ROM
  results (44 s runtime, all 17 `BASELINE` rows populated), not SKIPs.
* **The hardware rule itself was not re-litigated here** — that was out of scope — but one honesty
  flag: the `reg > 10` boundary comes from Kabuto's notes, which hedge it (*"the 10(?) SMS
  registers"*). VDPFIFOTesting test 12 pins **register 15** specifically. Masking of registers
  11-14 and 16-23, including the DMA registers 19-23, is extrapolation from the same sentence, not
  ROM-pinned. If someone wants that tightened before landing, that is a separate, small evidence
  task — it does not affect any measurement above.
* **What I did not measure:** whether landing T12 changes any *hand-checked* differential ROM
  (Gunstar / TF4 / Batman), which still have no automated conformance row. Those ROMs are real
  software and set M5, so the mask should be inert for them, but that is reasoning, not measurement.
* The real working tree was never modified. All three scratch trees live outside the repo.

---

## Recommendation

Land T12 as an ordinary slice. Suggested shape:

1. The one-line mask in `Vdp::write_register`, replacing the `NOT MODELLED` block at
   `crates/oracle-core/src/vdp.rs:613-621` with a short pinned-behavior note.
2. Un-`#[ignore]` `vdp::tests::mode4_ignores_register_writes_above_ten` and drop the stale
   "needs an owner ruling on the `GOLDEN_HASH` / `golden_frames` movement" reason string
   (`crates/oracle-core/src/vdp.rs:1374-1377`), which is now known to be false.
3. The 8-site fixture repair above — each site declaring Mode 5, which is what the fixture always
   meant.
4. `BASELINE` `vdp_port_access` → `14/2/16`, with the matching row in
   `docs/2026-07-25-testrom-conformance.md`.
5. Correct the two Decision-2 paragraphs in
   `docs/plans/2026-08-03-PARKED-owner-ruling.md` with a banner pointing here, closing the parked
   file out entirely.

No `export_state` version bump. No `GOLDEN_HASH` change. No `golden_frames` regeneration.
Nothing for the owner to rule on.
