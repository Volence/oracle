# Controller / I/O push — implementation plan

**Goal:** Replace the `$A10003–$A1001F` open-bus stub with a real Mega Drive I/O block — data/control
registers plus a 3-button pad device model — driven by a deterministic input-injection API, and prove a real
68000 fixture ROM can poll its pad through the TH protocol and visibly react.

**Architecture:** A new serialized `Io` struct (module `io.rs`), owned by `System` and borrowed into
`MegaDriveBus`, holds the three ports' data latches + control (direction) registers + serial-register stubs +
the injected pad state. Reads of a Data register are computed purely from that state per the recon IO3 model
`(latch & ctrl) | (device & !ctrl)`, where the device byte comes from the 3-button TH protocol (IO4). Input is
**injected state only** (`System::set_pad`) — zero host-input coupling. The block is byte-decoded in the bus
exactly where the stub is today.

**Tech stack:** Rust (`oracle-core`), `bincode::{Encode,Decode}` snapshots, the existing `Bus68k`/`MegaDriveBus`
adapter, `render_line` for the visible proof, PPM dumps for the owner's eyeball.

**Recon:** `docs/2026-07-17-io-recon.md` (IO1–IO6). Every register format and the pad protocol below trace to a
pin there.

---

## The currency-neutrality finding (why this push is safe)

Verified before planning:

- `crates/oracle-core/src/testrom.rs` (the ROM the export golden, determinism gate, and proptests all run)
  makes **zero** `$A1xxxx` accesses — `grep` clean. So nothing in the frozen currency ever reads the I/O block.
- `crates/oracle-core/tests/golden_frames.rs` builds `Vdp` directly and hashes `render_line` — no `System`, no
  CPU, no I/O. Immune to `Io` by construction.
- Oracle `state_hash` (`state_hash.rs`) hashes VDP regions only. `export_state` (`system.rs`) hashes
  version→regs→RAM→Z80→VDP→FM→PSG and does **not** gain `Io` this push.

Therefore **both goldens (export `0x22F80ECF29ED3AD4`, the 7 golden_frame hashes) and Oracle `state_hash`
stay byte-identical across every commit** — the new `Io` state is in **neither frozen currency** (an
export-v2 candidate, same disposition as the SAT cache), and only rides the internal bincode snapshot. This
is proven, isolated, at the field-adding commit (Slice 1) and re-confirmed after wiring (Slice 2).

`m68000/*` is untouched the entire push → SST is structurally invariant; run it once at HEAD and quote it,
stating the zero-diff.

---

## Decisions — surfaced, not defaulted

1. **6-button pad: OUT of scope (my call).** The 3-button read is a prefix of the 6-button read, so nothing
   here blocks it. The extension needs TH-toggle counting + a ~1.5 ms idle-timeout reset (master-clock
   coupling) for **no Sonic-era consumer** (S1/S2/S3K/S&K are 3-button). Deferred as a named follow-up; pin
   source = Plutiedev "Controllers" 6-button section (recon IO6). **Explicitly decided.**
2. **Serial registers (TxData / RxData / S-Control): deterministic stubs.** No serial peripheral is attached
   to a cartridge title. Model: **TxData** and **S-Control** read back the last byte written (retained
   latch); **RxData** reads `0x00` (no device driving the receive line). Writes are retained but drive
   nothing. Documented; real UART lands only if a serial peripheral is ever modelled. **Explicitly decided.**
3. **`Io` state in neither frozen currency** (per the push constraint) — export-v2 candidate, flagged in the
   struct doc-comment and the export-state spec's "deliberate exclusions". Round-trips the snapshot.
4. **Data-register read model** = `(latch & ctrl) | (device & !ctrl)` with `TH_line = ctrl.bit6 ? latch.bit6
   : 1` (input pins float high). Output-configured non-TH pins read back the latch (games rely on it); the
   open-collector button-pulls-output-low case is intentionally not modelled (recon IO3 remainder).
5. **Even byte addresses in the range read `0`** (the existing convention) — registers are odd-address low
   bytes on D0–D7.
6. **Z80 BUSREQ/RESET arbitration (`$A11100`/`$A11200`): out of scope**, flagged not picked up — lands with
   the Z80 core. Existing stub unchanged.

---

## File structure

- **Create** `crates/oracle-core/src/io.rs` — the `Io` struct, `Pad` struct, `set_pad`/`pad` helpers, the
  register-decode helper, the data-register read model, and their unit tests.
- **Modify** `crates/oracle-core/src/lib.rs` — `pub mod io;`.
- **Modify** `crates/oracle-core/src/system.rs` — add `io: Io` field, `set_pad`/`pad` public API, thread `io`
  through `mega_bus`.
- **Modify** `crates/oracle-core/src/bus.rs` — `MegaDriveBus` gains `io: &mut Io`; `mapped_byte`/`store_byte`
  route `$A10003–$A1001F` to the `Io` model instead of the stub.
- **Create** `crates/oracle-core/examples/pad_probe.rs` — dumps a before/after PPM pair (pad released vs
  Start held) for the owner.
- **Create** `crates/oracle-core/tests/io_controllers.rs` — the end-to-end integration test (inject → run →
  assert a rendered pixel changed).

---

## Data model (defined once, referenced by every slice)

```rust
// io.rs

/// One 3-button pad's button state. Injected state only — never touched by host input.
/// `true` = the button is held this instant. Serialized as part of the machine snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct Pad {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub b: bool,
    pub c: bool,
    pub a: bool,
    pub start: bool,
}

/// The Mega Drive I/O controller block ($A10003–$A1001F). Version ($A10001) stays a bus constant.
/// In NEITHER frozen currency (Oracle state_hash / export_state) — an export-v2 candidate, like the SAT
/// cache; it rides the bincode snapshot for determinism. Index 0 = Port 1, 1 = Port 2, 2 = EXP.
#[derive(Clone, Debug, Default, PartialEq, Eq, bincode::Encode, bincode::Decode)]
pub struct Io {
    /// Data-register latch per port (every written bit retained; only output pins drive — recon IO3).
    data: [u8; 3],
    /// Control (direction) register per port (bit=1 output, bit=0 input; bit7 = TH-IRQ enable — recon IO2).
    ctrl: [u8; 3],
    /// Serial TxData latch per port (stub: reads back last written — decision 2).
    txdata: [u8; 3],
    /// Serial S-Control latch per port (stub: reads back last written — decision 2).
    sctrl: [u8; 3],
    /// Injected pad state. Ports 0/1 only; EXP has no pad. Not driven by any host — recon IO4.
    pad: [Pad; 2],
}
```

Constants for the bit layout (defined once in `io.rs`, used by the device model and asserted by tests):
`TH_BIT = 6`, the two nibble maps of IO4.

---

## Slice 1 — `Io` state + injection API (the isolated field-add)

**Files:** create `io.rs`; modify `lib.rs`, `system.rs`. Bus **not** touched — the stub still answers the
range, so the currencies are provably unchanged by state addition alone.

**Traces to:** IO1 (register storage shape), IO2 (ctrl), decision 3 (neither currency), the snapshot policy.

- [ ] **Step 1 — write `io.rs`** with `Pad`, `Io`, and:
  ```rust
  impl Io {
      /// Inject pad state for port 0 (Player 1) or 1 (Player 2). EXP has no pad.
      pub fn set_pad(&mut self, port: usize, pad: Pad) {
          assert!(port < 2, "only ports 0 (P1) and 1 (P2) have a pad");
          self.pad[port] = pad;
      }
      /// The currently injected pad state for a port.
      pub fn pad(&self, port: usize) -> Pad {
          assert!(port < 2, "only ports 0 (P1) and 1 (P2) have a pad");
          self.pad[port]
      }
  }
  ```
  Add `pub mod io;` to `lib.rs`.

- [ ] **Step 2 — add the field + API to `System`.** In `system.rs`: add `io: Io` to the struct (after
  `vdp`), initialize `io: Io::default()` in `new()`, and expose:
  ```rust
  /// Inject 3-button pad state (Player 1 = port 0, Player 2 = port 1). Deterministic injected state;
  /// there is no host-input path. The next Data-register read reflects it (recon IO4).
  pub fn set_pad(&mut self, port: usize, pad: crate::io::Pad) {
      self.io.set_pad(port, pad);
  }
  /// The injected pad state for a port.
  pub fn pad(&self, port: usize) -> crate::io::Pad {
      self.io.pad(port)
  }
  ```

- [ ] **Step 3 — snapshot round-trip test** (in `io.rs` `#[cfg(test)]` at the `System` level, or in
  `system.rs` tests). Prove the injected state survives snapshot/restore:
  ```rust
  #[test]
  fn io_state_round_trips_a_snapshot() {
      let mut sys = System::new(1);
      sys.set_pad(0, Pad { start: true, up: true, ..Default::default() });
      sys.set_pad(1, Pad { a: true, ..Default::default() });
      let snap = sys.snapshot();
      let restored = System::restore(&snap).unwrap();
      assert_eq!(restored.pad(0), Pad { start: true, up: true, ..Default::default() });
      assert_eq!(restored.pad(1), Pad { a: true, ..Default::default() });
  }
  ```

- [ ] **Step 4 — prove currency neutrality, isolated.** Run, and record the numbers in the commit body:
  - `cargo test -p oracle-core --release --test export_state_v1` → the `0x22F80ECF29ED3AD4` guard passes.
  - `cargo test -p oracle-core --release --test golden_frames` → all 7 hashes unchanged.
  - `cargo test -p oracle-core --release --test determinism_gate` and `--test proptests` → green.
  - `cargo test -p oracle-core --release` (lib) → new count, +1 (round-trip test) plus any io.rs unit tests.
  - `git diff HEAD -- crates/oracle-core/src/state_hash.rs` → empty.

- [ ] **Step 5 — fmt + clippy + commit.**
  ```bash
  cargo fmt --check && cargo clippy --all-targets -- -D warnings
  git add -A && git commit -m "feat(io): serialized Io block + set_pad injection API (neither currency)"
  ```

## Slice 2 — wire the real register model into the bus (TDD)

**Files:** modify `bus.rs`, `system.rs` (thread `io` into `mega_bus`). This is where behavior at
`$A10003–$A1001F` changes from the `0x00` stub to the real model. Currencies still hold (testrom never reads
the range — re-confirmed at Step 6).

**Traces to:** IO2 (direction), IO3 (read model), IO4 (3-button device), IO5 (version untouched), decision 2
(serial stubs), decision 5 (even bytes read 0).

- [ ] **Step 1 — the device model + register decode in `io.rs`** (pure, unit-testable without the bus):
  ```rust
  /// The byte a 3-button pad drives given the TH line it sees (recon IO4). Active-low: pressed = 0.
  /// Bit 7 and (when TH high) bit 6 float high (pull-up); TH is echoed by the console's own latch via
  /// the IO3 read model, not by the device, so the device's bit 6 here is masked out for output TH.
  fn pad_device_byte(pad: Pad, th_high: bool) -> u8 {
      let lo = |pressed: bool| if pressed { 0 } else { 1 }; // active-low
      if th_high {
          0b1100_0000
              | (lo(pad.c) << 5)
              | (lo(pad.b) << 4)
              | (lo(pad.right) << 3)
              | (lo(pad.left) << 2)
              | (lo(pad.down) << 1)
              | lo(pad.up)
      } else {
          // bits 3,2 forced low (the MD-pad detection signature); bit 7 pull-up; bit 6 = TH line (0 here).
          0b1000_0000
              | (lo(pad.start) << 5)
              | (lo(pad.a) << 4)
              | (lo(pad.down) << 1)
              | lo(pad.up)
      }
  }

  impl Io {
      /// Read a Data register (recon IO3): output pins from the latch, input pins from the device.
      /// `port` in 0..3 (EXP has no pad → device byte is all-released).
      pub fn read_data(&self, port: usize) -> u8 {
          let ctrl = self.ctrl[port];
          let latch = self.data[port];
          let th_high = if ctrl & (1 << TH_BIT) != 0 { latch & (1 << TH_BIT) != 0 } else { true };
          let device = if port < 2 { pad_device_byte(self.pad[port], th_high) } else { pad_device_byte(Pad::default(), th_high) };
          (latch & ctrl) | (device & !ctrl)
      }
      pub fn write_data(&mut self, port: usize, byte: u8) { self.data[port] = byte; }
      pub fn read_ctrl(&self, port: usize) -> u8 { self.ctrl[port] }
      pub fn write_ctrl(&mut self, port: usize, byte: u8) { self.ctrl[port] = byte; }
      // txdata/sctrl accessors (read back latch); rxdata reads 0.
  }
  ```
  Plus a `pub fn io_reg(addr: u32) -> Option<IoReg>` mapping an odd address in `$A10003..=$A1001F` to a
  `(port, kind)` enum (`Data`/`Ctrl`/`TxData`/`RxData`/`SCtrl`). Version `$A10001` is **not** in this map.

- [ ] **Step 2 — unit tests in `io.rs`, one per pin** (write BEFORE Step 3; run and watch them fail). Each
  asserts through `read_data`, never by poking fields:
  - `th_high_reports_cbrlud`: ctrl=`$40`, latch=`$40`, inject `c+right` on P1 → `read_data(0)` has bit5=0,
    bit3=0, others (b/left/down/up) =1, and the detection bits behave.
  - `th_low_reports_start_a_and_forces_bits_2_3_low`: ctrl=`$40`, latch=`$00`, inject `start` → bit5=0,
    bit4=1, **bit3=0, bit2=0**, bit1/bit0=1.
  - `active_low_all_released_reads_high`: no buttons, ctrl=`$40`, latch=`$40` → low six bits all 1.
  - `input_pins_ignore_the_latch_output_pins_return_it`: ctrl=`$40` (only TH out) → the button bits come
    from the device regardless of latch; set ctrl=`$7F` (all out) latch=`$2A` → `read_data` low7 == `$2A`.
  - `exp_port_has_no_pad`: `read_data(2)` with ctrl=`$40` latch=`$40` → all-released device byte.
  - `serial_txdata_and_sctrl_read_back_last_write__rxdata_reads_zero`.
  - `io_reg_maps_every_documented_address` (table-drives the 15 addresses of IO1; `$A10001` → None).

- [ ] **Step 3 — thread `io` into the bus.** `system.rs::mega_bus` split-borrows `&mut self.io` and passes it
  to `MegaDriveBus::new`; add the `io: &'a mut Io` field + ctor param in `bus.rs`. Update the doc-comment map
  row for `$A10000–$A1001F`.

- [ ] **Step 4 — route the range in `mapped_byte`/`store_byte`.** In `mapped_byte`, replace the
  `0xA1_0000..=0xA1_001F => Some(if a==0xA1_0001 {MD_VERSION} else {0})` arm with: version at `$A10001`
  stays; for an odd address matched by `io_reg`, return `Some(self.io.read_data/read_ctrl/read_txdata/…)`;
  RxData → `Some(0)`; any even/unmatched byte in range → `Some(0)`. In `store_byte`, add: an odd address
  matched by `io_reg` → `self.io.write_*`. (Reads are pure → `mapped_byte`'s `&self` is fine; the bus
  already holds `&mut Io` so `store_byte`'s `&mut self` reaches it.)

- [ ] **Step 5 — bus-level test** in `bus.rs` `#[cfg(test)]`: drive the read through the actual
  `MegaDriveBus::read8`/`write8` path (not `Io` directly), mirroring `version_register_returns_the_fixed_constant`:
  ```rust
  #[test]
  fn port1_data_reads_the_injected_pad_through_the_th_protocol() {
      let mut h = Harness::new(); // whatever the file's fixture is
      // configure P1: TH output
      h.bus(|b| { b.write8(0xA1_0009, 5, 0x40); });
      // inject Start on P1 via the owning Io …
      // TH=1 read then TH=0 read; assert C/B/… then Start/A per IO4
  }
  ```
  (Use the file's existing harness shape; inject via the `Io` the harness owns.) Also assert the version
  register at `$A10001` still returns `MD_VERSION` (no regression).

- [ ] **Step 6 — re-prove currencies + gate.** Same commands as Slice 1 Step 4; record that export golden,
  golden_frames, determinism gate, proptests, and `state_hash` diff are all unchanged **after** the behavior
  change (the load-bearing re-confirmation). `git diff HEAD~1 -- crates/oracle-core/src/m68000` → empty.

- [ ] **Step 7 — fmt + clippy + commit.**
  ```bash
  cargo fmt --check && cargo clippy --all-targets -- -D warnings
  git add -A && git commit -m "feat(io): real data/control registers + 3-button TH pad model in the bus"
  ```

## Slice 3 — end-to-end proof: fixture ROM + injection test + before/after PPM

**Files:** create `tests/io_controllers.rs`, `examples/pad_probe.rs`. A **new** scene — it does not touch the
7 golden_frames scenes (their hashes stay pinned; stated purpose: exercise the full CPU→bus→Io→VDP→render
path under real pad polling).

**Traces to:** IO2/IO3/IO4 (the ROM polls through the real protocol), the whole-machine determinism path.

- [ ] **Step 1 — author the pad-polling fixture ROM** (in `tests/io_controllers.rs`, mirroring
  `examples/frame_dump.rs`'s `w`/`l`/`vdp_cmd` authoring helpers and its VDP-setup prologue). The ROM:
  1. Reset vectors (SSP = `$00FFFFFE`, PC = `$200`).
  2. VDP setup: enable display, a minimal palette, one solid opaque tile on plane A so the screen has a
     definite backdrop-vs-plane picture (reuse frame_dump's minimal setup).
  3. Controller init: `move.b #$40,($A10009)` (P1 TH output).
  4. Main loop (runs every frame): write `$40` to `$A10003`, read `$A10003` → mask **Start** is on the TH=0
     nibble, so write `$00`, read `$A10003`, test **bit 5 (Start)**. Active-low: Start held → bit5 = 0.
  5. Visible reaction: if Start is held, set **backdrop colour register** (VDP reg 7) to a palette index whose
     CRAM colour differs from the released state; else the default. `STOP` at loop end each frame, or a
     `DBRA` spin so `run_frames` advances. (Backdrop is the most robust single-pixel discriminator.)

- [ ] **Step 2 — the injection test** (write first; it fails until the ROM + wiring are correct):
  ```rust
  #[test]
  fn holding_start_changes_the_backdrop_via_the_real_pad_protocol() {
      let rom = pad_poll_fixture();
      // released
      let mut sys = System::new(1);
      sys.load_rom(rom.clone());
      sys.reset();
      sys.run_frames(3);
      let released = sys.vdp().render_line(100)[0]; // backdrop pixel (RGB)
      // held
      let mut sys2 = System::new(1);
      sys2.load_rom(rom);
      sys2.reset();
      sys2.set_pad(0, Pad { start: true, ..Default::default() });
      sys2.run_frames(3);
      let held = sys2.vdp().render_line(100)[0];
      assert_ne!(released, held, "holding Start must change the backdrop the ROM selected");
  }
  ```
  Pick line/column 100/0 to be a pure-backdrop dot (no plane A cell there), confirmed against the fixture's
  nametable.

- [ ] **Step 3 — run it, iterate the ROM until green.** `cargo test -p oracle-core --release --test
  io_controllers`. Debug with the CPU disassembly comments the way frame_dump was authored.

- [ ] **Step 4 — the `pad_probe` example** (owner's before/after picture). Boots the same fixture, dumps
  `pad_released.ppm` and `pad_start.ppm` (inject Start before the second run), so a glance shows the backdrop
  flip. Model it on `frame_dump.rs`'s PPM writer.
  ```bash
  cargo run -p oracle-core --release --example pad_probe
  # writes pad_released.ppm + pad_start.ppm; different backdrop colour = the pad reached the screen
  ```

- [ ] **Step 5 — golden_frames unchanged + gate.** Confirm the 7 golden hashes still pass (new scene is
  isolated). fmt + clippy.
  ```bash
  cargo fmt --check && cargo clippy --all-targets -- -D warnings
  git add -A && git commit -m "feat(io): end-to-end pad-poll fixture + injection test + before/after PPM"
  ```

## Final — full-gate self-verify at HEAD

- [ ] Run the whole gate and **quote actual output** in the report:
  - `cargo fmt --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo test -p oracle-core --release` (lib count)
  - `cargo test -p oracle-core --release --test determinism_gate --test proptests --test export_state_v1
    --test golden_frames --test oracle_differential`
  - `cargo test --workspace --release` (SST 112 / `ran >= 1_000_058`)
- [ ] `git diff 3b0853b..HEAD -- crates/oracle-core/src/m68000` → **empty** (SST invariance proof).
- [ ] Confirm the SST threshold line + harness (`tests/singlestep_m68000.rs`) and `FlatBus` untouched.
- [ ] Confirm export golden `0x22F80ECF29ED3AD4` + the 7 golden-frame hashes + `state_hash.rs` byte-identical
  vs `3b0853b`.

---

## Anti-cheating invariants (verifier-enforced)

- **Read the pad through the bus, never through `Io` internals.** Every read assertion (unit, bus, and
  integration) goes through `read_data`/`MegaDriveBus`/the CPU ROM. No test asserts a read value by peeking
  `io.data`/`io.pad`.
- **The device model is derived from injected state**, not hard-coded per test — `pad_device_byte` is one
  function; tests vary the injection and assert the derived byte.
- **The end-to-end test drives the real CPU** through `run_frames` polling the real protocol, and asserts a
  **rendered pixel**, not an `Io` field.
- **Currencies frozen:** export golden `0x22F80ECF29ED3AD4`, the 7 golden-frame hashes, and `state_hash.rs`
  are byte-identical at every commit; no golden regen. `Io` is added to neither.
- **`m68000/*` diff = 0**, SST threshold `ran >= 1_000_058` unchanged, SST harness + `FlatBus` untouched.
- **No floats.** Clean-room absolute (recon cites only permitted sources). Every commit fmt-clean, clippy
  `-D` including examples/tests. Conventional commits, no Co-Authored-By trailer. `../oracle/` never touched.

## Risks

- **R1 — a frozen-currency ROM reads the I/O range.** Mitigated: verified `testrom.rs` is `$A1xxxx`-clean and
  golden_frames renders `Vdp` directly; re-confirmed by re-running both goldens after the Slice-2 behavior
  change. If a future testrom change adds an I/O read, the export golden would move and this invariant breaks
  — flagged for the reviewer.
- **R2 — the default-pad read is `0xFF`-ish, not the old `0x00`.** A real released pad with `ctrl=0` reads
  high (pull-ups) where the stub read `0`. Currency-safe (unread), but it **is** an observable behavior
  change at `$A10003–$A1001F`; documented in the bus doc-comment and the recon IO3 remainder.
- **R3 — picking a pure-backdrop pixel for the E2E assertion.** Mitigated: choose the sample dot against the
  fixture's own nametable (a cell the fixture leaves as backdrop); assert inequality, not an absolute colour,
  so it is robust to the exact palette.
- **R4 — clippy on the bit math.** Mitigated: `pad_device_byte` uses explicit shifts + named bit constants;
  run `clippy --all-targets -D` per commit.
