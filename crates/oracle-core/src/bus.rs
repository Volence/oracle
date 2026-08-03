//! The typed `Bus` protocol + the `SystemBus` split-borrow adapter.
//!
//! Chips never touch memory directly; each step they borrow a transient `&mut SystemBus` (only one
//! `&mut` live at a time, monomorphized, zero dispatch — no `Rc`/`RefCell`/raw pointers). Every access
//! emits a [`BusEvent`] to a sink, so instrumentation (watchpoints, decoders, the profiler) is an
//! event-stream *consumer* rather than a CPU special-case. Re-entrant cross-chip writes go through one
//! explicit deferred-write seam: such writes are queued and drained by [`SystemBus::apply_writes`]
//! after the access completes (jgenesis's `MainBusWrites` pattern, reimplemented).

use crate::io::{io_reg, Io, IoReg};
use crate::state_hash::VRAM_SIZE;
use crate::system::RAM_SIZE;
use crate::vdp::Vdp;
use crate::ym2612::Ym2612;

/// 68000 work-RAM window base (`$FF0000`).
pub const RAM_BASE: u32 = 0xFF_0000;
/// Phase-0 synthetic VRAM window base. Lets the stub chip exercise the deferred-write seam through the
/// same `Bus` interface; the real VDP data-port semantics replace this when the VDP lands.
pub const VRAM_BASE: u32 = 0x10_0000;

/// Access width — byte, word, or long; the Genesis bus is big-endian (most-significant byte at the lowest
/// address). This is the SINGLE definition shared with the 68000 core: it is `m68000::microop::Size`
/// (bincode-serialized, used by every recipe + the `Bus68k` `Transaction` stream), re-exported here so the
/// generic bus layer and the CPU name the exact same type.
pub use crate::m68000::microop::Size;

/// What kind of bus access this is. `Tas` is the 68000's indivisible test-and-set read-modify-write —
/// ONE locked bus cycle, distinct from a separate `Read`+`Write` pair — so a consumer can tell the atomic
/// RMW apart from an ordinary access (it is also the access whose write the Mega Drive bus drops).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BusOp {
    Read,
    Write,
    Tas,
}

/// One memory access, emitted per bus operation. `value` is the value read or (requested to be) written.
/// `fc` is the 68000 function code that drove the access (5 = supervisor data, 6 = supervisor program,
/// etc.); non-CPU masters (DMA, later chips) emit `fc = 0`, so instrumentation can attribute every access
/// to its master and space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BusEvent {
    pub op: BusOp,
    pub fc: u8,
    pub addr: u32,
    pub size: Size,
    pub value: u32,
}

/// A consumer of the bus event stream (watchpoints, recorders, decoders, the profiler...).
pub trait BusEventSink {
    fn on_event(&mut self, event: BusEvent);

    /// Timestamped delivery: the same event plus the absolute master-clock (mclk) of the access.
    /// Emission sites that hold the current mclk (the real 68k/Z80 buses) call THIS; the default
    /// forwards to `on_event`, so every existing sink is behaviorally unchanged and needs no edit.
    /// Only a timing-aware sink (the synth AudioSink, SY-4b) overrides it.
    fn on_event_at(&mut self, event: BusEvent, _mclk: u64) {
        self.on_event(event);
    }

    /// Called by the sink-generic run loop immediately before each CPU step, stamping the PC of the
    /// instruction about to execute and the current frame. Consumers that attribute an access to its writing
    /// instruction (watchpoints) latch this context so each subsequent [`BusEvent`] knows its PC/frame; a
    /// `BusEvent` carries no PC of its own (it is emitted per-access deep in the CPU), so this step-boundary
    /// stamp is where the instruction identity enters the stream. The default is a no-op, so the null-sink
    /// hot path (`()`) and the recording sink (`Vec<BusEvent>`) are behaviorally unchanged.
    fn on_step_boundary(&mut self, _pc: u32, _frame: u64) {}

    /// Whether this sink wants VDP-internal writes delivered (watchpoints v2). The sink-generic run loop calls
    /// this **once per run** and, only if it returns `true`, arms the VDP's write-capture buffer for the run —
    /// so the currency-sensitive capture path stays byte-for-byte off unless a consumer opts in. The default is
    /// `false`, so `()` / `Vec<BusEvent>` never arm capture.
    fn wants_vdp_writes(&self) -> bool {
        false
    }

    /// A VDP-internal memory write (VRAM/CRAM/VSRAM), delivered after the driving CPU step when this sink's
    /// [`wants_vdp_writes`](BusEventSink::wants_vdp_writes) returned `true` (watchpoints v2). Carries the
    /// resolved region address, old→new value, size, and CPU-vs-DMA attribution; a consumer pairs it with the
    /// most recent [`on_step_boundary`](BusEventSink::on_step_boundary) PC. The default is a no-op.
    fn on_vdp_write(&mut self, _write: crate::vdp::VdpWrite) {}

    /// Whether this sink wants rendered scanlines delivered (conformance Limitation L1: mid-frame CRAM /
    /// palette effects are invisible to an after-the-run capture). The `Scanline` event queries this per
    /// active line and, only when `true`, decodes the already-built line report to RGB for
    /// [`on_scanline`](BusEventSink::on_scanline) — the default `false` keeps the null-sink path exactly the
    /// discard-the-render hot path (no decode, no allocation).
    fn wants_scanlines(&self) -> bool {
        false
    }

    /// One rendered active line (0..=223), delivered **during** the run at the moment the self-rescheduling
    /// `Scanline` event renders it — so the RGB reflects the VDP state (CRAM included) live at that line, not
    /// the end-of-frame state. `rgb` is a borrow of the line just rendered (length = the active width, 256
    /// H32 / 320 H40); copy out whatever must outlive the call. Only called when
    /// [`wants_scanlines`](BusEventSink::wants_scanlines) returned `true`. The default is a no-op.
    fn on_scanline(&mut self, _line: u16, _rgb: &[(u8, u8, u8)]) {}
}

/// Null sink — discards events (the hot path, with no instrumentation attached).
impl BusEventSink for () {
    fn on_event(&mut self, _event: BusEvent) {}
}

/// Recording sink — captures the full access stream (tests, tracing).
impl BusEventSink for Vec<BusEvent> {
    fn on_event(&mut self, event: BusEvent) {
        self.push(event);
    }
}

/// The typed bus protocol a chip uses to access the machine.
pub trait Bus {
    /// Read `size` bytes at `addr` (big-endian), emitting a read event.
    fn read(&mut self, addr: u32, size: Size) -> u32;
    /// Write `value` (`size` bytes, big-endian) at `addr`, emitting a write event. The write may be
    /// applied immediately or deferred depending on the target.
    fn write(&mut self, addr: u32, size: Size, value: u32);
}

/// Split-borrow adapter: borrows the `System`'s memory fields + an event sink for the duration of one
/// chip step, plus a private deferred-write queue.
pub struct SystemBus<'a, S: BusEventSink> {
    ram: &'a mut [u8],
    vram: &'a mut [u8],
    sink: &'a mut S,
    deferred: Vec<(u32, Size, u32)>,
}

impl<'a, S: BusEventSink> SystemBus<'a, S> {
    /// Build an adapter over the given memory regions and event sink.
    pub fn new(ram: &'a mut [u8], vram: &'a mut [u8], sink: &'a mut S) -> Self {
        Self {
            ram,
            vram,
            sink,
            deferred: Vec::new(),
        }
    }

    fn is_vram(addr: u32) -> bool {
        (VRAM_BASE..VRAM_BASE + VRAM_SIZE as u32).contains(&addr)
    }

    /// Resolve `addr` to its backing slice, the region base, and the index mask.
    fn slice_and_base(&mut self, addr: u32) -> (&mut [u8], u32, usize) {
        if Self::is_vram(addr) {
            (self.vram, VRAM_BASE, VRAM_SIZE - 1)
        } else {
            (self.ram, RAM_BASE, RAM_SIZE - 1)
        }
    }

    fn read_raw(&mut self, addr: u32, size: Size) -> u32 {
        let (buf, base, mask) = self.slice_and_base(addr);
        let n = size.bytes();
        let mut value = 0u32;
        for i in 0..n {
            let idx = (addr.wrapping_add(i).wrapping_sub(base) as usize) & mask;
            value = (value << 8) | buf[idx] as u32;
        }
        value
    }

    fn write_raw(&mut self, addr: u32, size: Size, value: u32) {
        let (buf, base, mask) = self.slice_and_base(addr);
        let n = size.bytes();
        for i in 0..n {
            let shift = 8 * (n - 1 - i);
            let byte = ((value >> shift) & 0xFF) as u8;
            let idx = (addr.wrapping_add(i).wrapping_sub(base) as usize) & mask;
            buf[idx] = byte;
        }
    }

    /// Drain the deferred-write queue into memory. Called once after each chip step.
    pub fn apply_writes(&mut self) {
        let pending = std::mem::take(&mut self.deferred);
        for (addr, size, value) in pending {
            self.write_raw(addr, size, value);
        }
    }
}

impl<'a, S: BusEventSink> Bus for SystemBus<'a, S> {
    fn read(&mut self, addr: u32, size: Size) -> u32 {
        let value = self.read_raw(addr, size);
        self.sink.on_event(BusEvent {
            op: BusOp::Read,
            fc: 0,
            addr,
            size,
            value,
        });
        value
    }

    fn write(&mut self, addr: u32, size: Size, value: u32) {
        self.sink.on_event(BusEvent {
            op: BusOp::Write,
            fc: 0,
            addr,
            size,
            value,
        });
        if Self::is_vram(addr) {
            self.deferred.push((addr, size, value));
        } else {
            self.write_raw(addr, size, value);
        }
    }
}

// -----------------------------------------------------------------------------------------------------------
// MegaDriveBus — the CPU-facing `Bus68k` adapter over the real Mega Drive memory map.
// -----------------------------------------------------------------------------------------------------------

use crate::m68000::bus68k::{Bus68k, ADDR_MASK};
use crate::system::MCLK_PER_CPU_CYCLE;
use crate::vdp::{DmaMode, DmaRecord, DmaRequest, Target};

/// 8 KiB of Z80 RAM, visible to the 68000 at `$A00000` (mirrored across the 64 KiB `$A00000–$A0FFFF` window).
pub const Z80_RAM_SIZE: usize = 0x2000;

/// The byte returned from the version register at `$A10001` — a fixed placeholder (export/NTSC/no-expansion,
/// hardware version 0). Real region/timing + controller detection lands with the pads in a later phase.
pub const MD_VERSION: u8 = 0xA0;

/// A detected cartridge SRAM mapping (from the ROM's "RA" header): the inclusive bus-address span the SRAM
/// chip occupies and its byte-lane parity. `Copy` — the bus receives it by value each step; presence is
/// carried by the `Option<SramMap>` wrapper (`None` = no cart SRAM → pure ROM). Parsed by
/// `System::load_rom`; see `docs/2026-07-23-sram-design-recon.md` (§A3/A4, Fork 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SramMap {
    /// Inclusive base bus address of the SRAM window (header `$1B4-7`).
    pub base: u32,
    /// Inclusive end bus address of the SRAM window (header `$1B8-B`).
    pub end: u32,
    /// `true` = odd-byte cart (SRAM answers only odd bus addresses); `false` = even-byte.
    pub odd: bool,
}

/// Split-borrow adapter implementing the CPU-facing [`Bus68k`] over the `System`'s memory fields laid out per
/// the real Mega Drive map, emitting a [`BusEvent`] (with the real function code) per access. The CPU core
/// cannot tell it apart from the SST harness's `FlatBus` — the point of the unification. Every 24-bit address
/// has a deterministic answer; open bus returns the last word driven on the bus (`last_bus_word`). Writes
/// apply immediately in this pivot (no other master is live); the deferred-write seam plugs in with the VDP.
///
/// | Range | Behavior |
/// |---|---|
/// | `$000000–$3FFFFF` | ROM (read-only; past a short ROM's end → open bus) |
/// | `$400000–$7FFFFF` | open bus, arbiter flavor (residue high byte, low byte `$00` — K4-1) |
/// | `$A00000–$A0FFFF` | the Z80 window, masked to 15 bits (`$A08000+` behaves as `$A00000+`) and decoded per the Z80's own bus map (z80/bus.rs): `$0000-$3FFF` Z80 RAM, `$6000-$60FF` bank latch (write = serial tick of the shared register, read `$FF`), `$6100-$7EFF` `$FF`, `$7F00-$7FFF` VDP-port mirror (live status/HV via the shared K2 reader, PSG write tap at `$7F11`). Forwarded only while BUSREQ granted AND reset released (K4-3), else arbiter open bus / dropped writes. Word reads mirror the even byte into both halves; word WRITES land the high byte only (Q4) |
/// | `$A04000–$A05FFF` | YM2612 FM (window offset `$4000-$5FFF`, ports = low 2 bits): read = live status (bit7 BUSY clear); writes drive the timer model — answering regardless of bus ownership (K4-3 pin) |
/// | `$A10000–$A1001F` | I/O: `$A10001` = [`MD_VERSION`]; the 15 data/control/serial registers via [`Io`] |
/// | `$A11100` | Z80 BUSREQ: bit0 read = 0 when 68000 is granted the bus (asserted), 1 when the Z80 owns it |
/// | `$A11200` | Z80 RESET: bit0 = the reset-release latch (`z80_running`) — write 1 = release (Z80 runs), 0 = assert (held); WRITE-ONLY, reads are arbiter open bus (K4-1) |
/// | `$C00000`/`$C00002` | VDP data port (read = pre-cache buffer, write = VRAM/CRAM/VSRAM; recon R1) |
/// | `$C00004`/`$C00006` | VDP control port (read = status word, write = command; recon R1/R2) |
/// | `$C00008–$C0000F` | VDP HV counter (even byte = V, odd byte = H; recon R2) |
/// | `$E00000–$FFFFFF` | 64 KiB work RAM (mirrored) |
///
/// The VDP is borrowed here (`&mut Vdp`) alongside the master-clock reading (`now_mclk`) so the timing FSM
/// (h/v counters, status bits) reads live off the clock at access time.
pub struct MegaDriveBus<'a, S: BusEventSink> {
    rom: &'a [u8],
    ram: &'a mut [u8],
    z80_ram: &'a mut [u8],
    vdp: &'a mut Vdp,
    io: &'a mut Io,
    now_mclk: u64,
    last_bus_word: &'a mut u16,
    /// The Z80 BUSREQ latch: `true` once the 68000 has written bit0 = 1 to `$A11100` (bus requested → granted),
    /// `false` after a release (bit0 = 0). Read back at `$A11100` bit0 as 0 when granted to the 68000, 1 when
    /// the Z80 owns the bus — the take-bus/release handshake real games spin on (DR-1 Gunstar). Bus-internal
    /// state, threaded like `last_bus_word`; NOT in the frozen `export_state`. Semantics + evidence:
    /// `docs/2026-07-22-z80-busreq-recon.md` (Z2/Z5/Z6).
    z80_busreq: &'a mut bool,
    /// The Z80 RESET-release latch (`$A11200` bit0): `true` = reset released (Z80 runs), `false` = reset
    /// asserted (Z80 held). **Power-on = `false`** — real hardware holds the Z80 in reset until the 68000
    /// releases it (Plutiedev "Using the Z80"). Stored positively (`z80_running`) to avoid the reset-polarity
    /// foot-gun. This slice (Z-skeleton) promotes it from the old constant-0/drop stub to a real latch, but
    /// nothing releases it in any committed fixture, so the Z80 executes zero instructions. Bus-internal +
    /// bincode-serialized like `z80_busreq`; NOT in `export_state`. See `docs/2026-07-22-z80-core-design.md`
    /// (ZC6/ZC13).
    z80_running: &'a mut bool,
    /// The Z80's 9-bit `$6000` bank latch — the SAME `System::z80_bank` scalar the Z80-side bus borrows
    /// (one physical register, two paths): a 68k write into the open window at Z80 offset `$6000-$60FF`
    /// ticks it through [`crate::z80::bus::bank_latch_tick`], exactly like the Z80's own `$6000` write
    /// (Oracle `MDBusArbiter.cpp` `Z80WindowBankswitch` — reached from both buses). Threaded like
    /// `z80_busreq`; NOT in `export_state`.
    z80_bank: &'a mut u16,
    /// The cartridge SRAM-access-enable latch (`$A130F1` bit0): `true` once a game writes bit0 = 1 (SRAM
    /// mapped at `$200001+`), `false` after bit0 = 0 (ROM shown). Latched from the ODD-byte write to
    /// `$A130F1` (the shipping S3K driver does `move.b #1,($A130F1)`; `skdisasm/sonic3k.asm:344`). S0 promotes
    /// the old drop-stub to a real latch but adds NO SRAM buffer and NO `$200000+` mapping change, so this
    /// scalar has no consumer yet — currency-neutral by construction. Threaded like `z80_busreq`; NOT in
    /// `export_state`/`state_hash`. Semantics: `docs/2026-07-23-sram-design-recon.md` (§"S0 — `$A130F1`").
    sram_enabled: &'a mut bool,
    /// The cartridge SRAM write-protect latch (`$A130F1` bit1): `true` = SRAM read-only. Convention-pinned
    /// (no in-tree driver exercises it) and latched now so S1's buffer honors it without a second bus change.
    /// Bus-internal + bincode-serialized like `sram_enabled`; NOT in `export_state`. See the design recon.
    sram_write_protect: &'a mut bool,
    /// The live cartridge SRAM bytes (empty when no cart declared SRAM). A visible SRAM read/write indexes
    /// this by `(a - base) >> 1` (the every-other-byte wiring, §A4). Split-borrowed like `z80_ram`; rides the
    /// bincode snapshot but is NOT in `export_state` (S3) / `state_hash`. See the design recon (§B5-B7, Fork 5).
    sram: &'a mut [u8],
    /// Set `true` on any guest write into visible SRAM — the frontend's S2 persistence throttle. Threaded
    /// like the latches; a non-currency scalar (in the snapshot for determinism, out of the frozen currencies).
    sram_dirty: &'a mut bool,
    /// Latched `true` the first time the guest writes visible SRAM and **never cleared here** (the S4
    /// "this cart actually uses SRAM" signal, distinct from the debounce-cleared `sram_dirty`). The frontend
    /// gates `.srm` creation on it so the header-less fallback map never fabricates a save file for a cart
    /// that only ever reads (or ignores) SRAM. Non-currency scalar, snapshot-only. See the design recon (S4).
    sram_used: &'a mut bool,
    /// The detected SRAM map (`None` = no cart SRAM → the `$000000-$3FFFFF` region is pure ROM, currency-
    /// neutral). When `Some`, SRAM overlays ROM only while `sram_enabled` and the address is in range with the
    /// matching parity (see [`MegaDriveBus::sram_index`]). `Copy`, passed by value each step.
    sram_map: Option<SramMap>,
    /// The YM2612 FM chip (its timers, this slice): a `$A04000-$A04003` read returns its status byte (Timer-A/B
    /// overflow flags live, bit7 BUSY clear), and a write drives the address-latch/data protocol into its timer
    /// model. Split-borrowed like `vdp`; rides the bincode snapshot but is NOT in `export_state`. See
    /// `docs/2026-07-22-fm-timer-design.md`.
    fm: &'a mut Ym2612,
    sink: &'a mut S,
}

impl<'a, S: BusEventSink> MegaDriveBus<'a, S> {
    /// Build an adapter over the given memory regions, the VDP + the master-clock reading, the I/O block,
    /// the open-bus latch, the FM chip, and an event sink.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        rom: &'a [u8],
        ram: &'a mut [u8],
        z80_ram: &'a mut [u8],
        vdp: &'a mut Vdp,
        io: &'a mut Io,
        now_mclk: u64,
        last_bus_word: &'a mut u16,
        z80_busreq: &'a mut bool,
        z80_running: &'a mut bool,
        z80_bank: &'a mut u16,
        sram_enabled: &'a mut bool,
        sram_write_protect: &'a mut bool,
        sram: &'a mut [u8],
        sram_dirty: &'a mut bool,
        sram_used: &'a mut bool,
        sram_map: Option<SramMap>,
        fm: &'a mut Ym2612,
        sink: &'a mut S,
    ) -> Self {
        Self {
            rom,
            ram,
            z80_ram,
            vdp,
            io,
            now_mclk,
            last_bus_word,
            z80_busreq,
            z80_running,
            z80_bank,
            sram_enabled,
            sram_write_protect,
            sram,
            sram_dirty,
            sram_used,
            sram_map,
            fm,
            sink,
        }
    }

    /// The SRAM buffer index a `$000000-$3FFFFF` access resolves to, or `None` when SRAM is not visible at
    /// `a`. Visible iff a map is present, the game has enabled SRAM via `$A130F1` bit0, `a` is inside the
    /// mapped range, **and** `a`'s parity is the chip's byte lane (odd-byte carts answer only odd addresses;
    /// the unused parity falls through to ROM/open-bus). The index is `(a - base) >> 1` (§A4/Fork 5).
    fn sram_index(&self, a: u32) -> Option<usize> {
        let m = self.sram_map?;
        (*self.sram_enabled && a >= m.base && a <= m.end && ((a & 1) == 1) == m.odd)
            .then(|| ((a - m.base) >> 1) as usize)
    }

    /// The byte backing a mapped address, or `None` for open bus (unmapped ranges, past a short ROM's end,
    /// the VDP data port). Real memory (ROM / work RAM / Z80 RAM) and the fixed-constant registers return
    /// `Some`; the caller substitutes the open-bus latch for `None`. Takes `&mut self` because the Z80
    /// window's VDP-mirror arm is a real side-effecting status read (`$A07F04+` clears the control-port
    /// write-toggle, exactly like the Z80's own `$7F04` read — K2's 68k-side half).
    fn mapped_byte(&mut self, a: u32) -> Option<u8> {
        match a {
            // Cartridge address space. SRAM overlays ROM only when a cart declared SRAM AND the game has
            // enabled it (`$A130F1` bit0) AND `a` is in range with the matching parity — otherwise this is
            // ROM exactly as before (open bus past a short ROM's end, no mirroring). `sram_index` is `None`
            // for every golden (no "RA" header, no `$A130F1` write) → byte-identical ROM decode.
            0x00_0000..=0x3F_FFFF => {
                if let Some(i) = self.sram_index(a) {
                    Some(self.sram[i])
                } else {
                    let i = a as usize;
                    (i < self.rom.len()).then(|| self.rom[i])
                }
            }
            // The 68k-side Z80 window ($A00000-$A0FFFF): the window's address is masked to 15 bits
            // (MDBusArbiter.cpp:304 — Charles MacDonald's hardware tests; $A08000-$A0FFFF behaves as
            // $A00000-$A07FFF), then decoded per the Z80's OWN local bus map (z80/bus.rs — one source of
            // truth). YM2612 first ($4000-$5FFF, the chip's full select span — memtest row 4 pins
            // `A04000-A05FFF = 0000` = the status byte): a READ returns the live status byte — Timer-A
            // overflow (bit0), Timer-B (bit1), bit7 BUSY always clear (recon F2/F4, DR-1b Gunstar) —
            // answering regardless of bus ownership (the K4-3 adjudicated pin, design §4 row 4).
            // Everything else is forwarded only while the 68k owns the Z80 bus — BUSREQ granted AND
            // reset released (K4-3; MDBusArbiter.cpp:482; memtest row 2 reads open bus with BUSREQ held
            // under reset). Closed -> None (arbiter open bus / dropped writes). Open:
            //   $0000-$3FFF -> 8 KiB Z80 RAM mirrored;
            //   $6000-$7EFF -> $FF (write-only bank register + unused; row 5, MDBusArbiter.cpp:422-437);
            //   $7F00-$7FFF -> the SAME VDP-port mirror the Z80 reads (K2's 68k-side half): live status
            //                  at $7F04-$7F07 (side-effecting), live HV at $7F08-$7F0F, $FF elsewhere
            //                  (data port = the ledgered lockup known-difference).
            0xA0_0000..=0xA0_FFFF => {
                let z = (a & 0x7FFF) as u16;
                if let 0x4000..=0x5FFF = z {
                    return Some(self.fm.read_status(self.now_mclk));
                }
                if !(*self.z80_busreq && *self.z80_running) {
                    None
                } else {
                    Some(match z {
                        0x0000..=0x3FFF => self.z80_ram[z as usize & (Z80_RAM_SIZE - 1)],
                        0x7F00..=0x7FFF => {
                            crate::z80::bus::vdp_mirror_read(self.vdp, z, self.now_mclk)
                        }
                        // $6000-$7EFF: the bank register is write-only, the rest unused — all-ones.
                        _ => 0xFF,
                    })
                }
            }
            // I/O ($A10000–$A1001F): the register block does not decode A0 (K4-4; Exodus
            // `AddressDiscardLowerBitCount="1"`, memtest row 6 `A0A0`), so each odd register answers BOTH
            // byte lanes — decode `a | 1`: the version byte at $A10000/1, the 15 data/control/serial
            // registers via the `Io` model (recon IO1–IO4). A word read is the register duplicated for free.
            0xA1_0000 | 0xA1_0001 => Some(MD_VERSION),
            0xA1_0002..=0xA1_001F => Some(match io_reg(a | 1) {
                Some((port, IoReg::Data)) => self.io.read_data(port),
                Some((port, IoReg::Ctrl)) => self.io.read_ctrl(port),
                Some((port, IoReg::TxData)) => self.io.read_txdata(port),
                Some((port, IoReg::SCtrl)) => self.io.read_sctrl(port),
                // RxData: no serial device drives the receive line (decision 2).
                Some((_, IoReg::RxData)) | None => 0x00,
            }),
            // Z80 BUSREQ ($A11100): partially decoded — the arbiter drives ONLY the grant bit (bit0 of this
            // even byte / bit8 of the word) and the odd byte to $00; bits 1-7 here float with the residue's
            // high byte (K4-2, memtest rows 7/8: `4F00`/`4E00`). The readable bit folds Z80 RESET in
            // (MDBusArbiter.cpp:444: reset || !busreq || !busgrant): 1 = "bus unavailable" — so it reads 1
            // while reset is asserted even with BUSREQ held, which is why real grant spins are bounded or
            // release reset first. `btst #0` take-bus/release spins (recon Z2/Z5, DR-1) see the same bit as
            // before once reset is released.
            0xA1_1100 => {
                let unavailable = !(*self.z80_busreq && *self.z80_running);
                Some(((*self.last_bus_word >> 8) as u8 & 0xFE) | unavailable as u8)
            }
            0xA1_1101 => Some(0x00),
            // Z80 RESET ($A11200): WRITE-ONLY — a read drives no data lines at all (the reference arbiter's
            // Z80RESET read handler returns nothing, MDBusArbiter.cpp:448-452), so it falls through to
            // arbiter-flavored open bus (K4-1; memtest row 9 pins `4E00` across reset toggles). The write
            // latch (`z80_running`) lives in `store_byte`.
            // VDP ports ($C00000–$C0000F): stateful, handled as whole accesses in read16/write16/read8/
            // write8 (a byte-wise decode here would double a port access's side effects). Open bus for any
            // fallthrough (e.g. a TAS against a port — never a real access).
            0xC0_0000..=0xC0_000F => None,
            // Work RAM: 64 KiB mirrored across $E00000–$FFFFFF.
            0xE0_0000..=0xFF_FFFF => Some(self.ram[(a as usize) & (RAM_SIZE - 1)]),
            // $400000–$7FFFFF and every gap: open bus.
            _ => None,
        }
    }

    /// Store `byte` if `a` lands in writable memory (work RAM / Z80 RAM). Writes to ROM, the I/O / Z80-control
    /// / VDP-port regions are accepted (they still drive the bus + emit an event) but not stored — the
    /// placeholder scope until those chips land.
    fn store_byte(&mut self, a: u32, byte: u8) {
        match a {
            // Cartridge address space. A write hits SRAM only when it is visible (map present + enabled +
            // in range + matching parity) AND not write-protected (`$A130F1` bit1); it then sets the dirty
            // flag for the frontend's persistence throttle. Every other write here — ROM, write-protected
            // SRAM, the unused parity, no-cart — is dropped exactly as ROM writes were before (currency-
            // neutral: `sram_index` is `None` for every golden). See the design recon (§A4, Fork 5).
            0x00_0000..=0x3F_FFFF => {
                if let Some(i) = self.sram_index(a) {
                    if !*self.sram_write_protect {
                        self.sram[i] = byte;
                        *self.sram_dirty = true;
                        // Latch "this cart uses SRAM" (never cleared here) — the frontend's S4 signal that a
                        // real save happened, so the header-less fallback map only births a `.srm` when the
                        // game actually wrote it (a read-only / SRAM-ignoring cart makes no file).
                        *self.sram_used = true;
                    }
                }
            }
            // The 68k-side Z80 window ($A00000-$A0FFFF), 15-bit-masked like the read side, decoded per the
            // Z80's own local bus map. YM2612 first ($4000-$5FFF, partially decoded to the 4 ports by the
            // low 2 bits): writes drive the FM chip's address-latch/data protocol (timer regs update the
            // timer model — recon F1/DR-1b, Option B), answering regardless of bus ownership like the read
            // side, and never falling through to the RAM store (that aliasing corrupted `z80_ram[0..3]`).
            // Everything else lands only while the window is open (K4-3 gate); closed writes drop:
            //   $0000-$3FFF -> Z80 RAM;
            //   $6000-$60FF -> one serial tick of the SAME 9-bit bank latch the Z80's own $6000 write
            //                  loads (`bank_latch_tick` — one register, two paths; Oracle
            //                  `Z80WindowBankswitch` is reached from both buses);
            //   $7F11       -> the PSG port through the mirror: tap the Z80-side-shaped BusEvent into the
            //                  sink (addr $7F11, fc 0 — the same event the Z80's own write emits, so the
            //                  VGM logger/synth unify the two paths at the register-file level);
            //   the rest of $6100-$7FFF (unused / write-only VDP mirror) drops, matching z80/bus.rs.
            0xA0_0000..=0xA0_FFFF => {
                let z = (a & 0x7FFF) as u16;
                if let 0x4000..=0x5FFF = z {
                    self.fm.write_port(0x4000 | (z & 3), byte, self.now_mclk);
                } else if *self.z80_busreq && *self.z80_running {
                    match z {
                        0x0000..=0x3FFF => self.z80_ram[z as usize & (Z80_RAM_SIZE - 1)] = byte,
                        0x6000..=0x60FF => crate::z80::bus::bank_latch_tick(self.z80_bank, byte),
                        0x7F11 => self.emit(BusOp::Write, 0, 0x7F11, Size::Byte, byte as u32),
                        _ => {}
                    }
                }
            }
            // Z80 BUSREQ ($A11100): latch bit0 from the EVEN byte only — a word write (`move.w #$100/#$0`)
            // puts the meaningful byte at $A11100 and 0 at $A11101, which must not clobber the latch. $A11101
            // and $A11200 (RESET, deferred to Z7) fall through and drop (recon Z1/Z5).
            0xA1_1100 => *self.z80_busreq = (byte & 1) != 0,
            // Z80 RESET ($A11200): latch bit0 from the EVEN byte only (a word write `move.w #$100,$A11200`
            // puts the meaningful byte at $A11200, 0 at $A11201). 1 = release reset (Z80 runs), 0 = assert
            // (held). $A11201 falls through and drops. No committed fixture writes bit0 = 1 (design ZC13).
            0xA1_1200 => *self.z80_running = (byte & 1) != 0,
            // Cartridge SRAM control ($A130F1, the "TIME" line's SRAM-access byte): latch bit0 = SRAM enable
            // (1 = SRAM mapped at $200001+, 0 = ROM shown) and bit1 = write-protect (1 = read-only). $A130F1
            // is the ODD byte of its word, and the shipping driver writes it directly with a byte store
            // (`move.b #1,($A130F1)`, S3K sonic3k.asm:344), so this arm sees the meaningful byte; a word write
            // to the even neighbour $A130F0 puts 0 here and falls through the same way $A11101 does for the
            // Z80 latch. Write-only register — there is deliberately NO read arm (reads stay open bus). S0 has
            // no SRAM buffer/$200000+ mapping yet (that is S1), so this latch is inert and currency-neutral.
            // Semantics: docs/2026-07-23-sram-design-recon.md (§"S0 — $A130F1 semantics").
            0xA1_30F1 => {
                *self.sram_enabled = (byte & 1) != 0;
                *self.sram_write_protect = (byte & 2) != 0;
            }
            // I/O register writes ($A10003–$A1001F): data/control latches + serial stubs (recon IO2/IO3).
            // The version byte and RxData are read-only; even bytes are unmapped. All drop here.
            0xA1_0000..=0xA1_001F => match io_reg(a) {
                Some((port, IoReg::Data)) => self.io.write_data(port, byte),
                Some((port, IoReg::Ctrl)) => self.io.write_ctrl(port, byte),
                Some((port, IoReg::TxData)) => self.io.write_txdata(port, byte),
                Some((port, IoReg::SCtrl)) => self.io.write_sctrl(port, byte),
                Some((_, IoReg::RxData)) | None => {}
            },
            0xE0_0000..=0xFF_FFFF => self.ram[(a as usize) & (RAM_SIZE - 1)] = byte,
            _ => {}
        }
    }

    fn emit(&mut self, op: BusOp, fc: u8, addr: u32, size: Size, value: u32) {
        self.sink.on_event_at(
            BusEvent {
                op,
                fc,
                addr,
                size,
                value,
            },
            self.now_mclk,
        );
    }

    /// Whether `a` (already masked) is a VDP port ($C00000–$C0000F).
    fn is_vdp_port(a: u32) -> bool {
        (0xC0_0000..=0xC0_000F).contains(&a)
    }

    /// The word an undriven ("open bus") read at `a` returns — the two open-bus flavors pinned by the
    /// memtest hardware column (K4 design `docs/2026-08-02-k4-openbus-design.md` §3):
    ///
    /// - **Arbiter-answered** regions — the cart-time gap `$400000-$7FFFFF` (row 1) and the write-only
    ///   `$A11200` reset register whose reads drive nothing (row 9) — return the residue's HIGH byte with
    ///   the low byte driven to `$00` (`4E00`). An empirical rule (design §6 Q1): the reference keeps the
    ///   full word here; the ROM's inline hardware column is our pinned ground truth.
    /// - Everything else (ROM past a short cart's end, VDP-side gaps like `$C00018`) retains the **full**
    ///   latch word — classic tri-state decay (row 13 `4E71`, already exact before K4).
    ///
    /// Callers: `read16`'s open arm, `read8`'s per-lane halves (an odd arbiter byte reads `$00` for free).
    /// The latch itself is never updated by an undriven read — nothing new crossed the bus.
    fn open_word(&self, a: u32) -> u16 {
        match a {
            // The Z80 window reaches here only while CLOSED (mapped_byte answers it when open) — a
            // closed-window read is arbiter-answered too (K4-3; memtest row 2 `4E00`).
            0x40_0000..=0x7F_FFFF | 0xA0_0000..=0xA0_FFFF | 0xA1_1200 | 0xA1_1201 => {
                *self.last_bus_word & 0xFF00
            }
            _ => *self.last_bus_word,
        }
    }

    /// A whole-word VDP port read (recon R1/R2), with its side effects (toggle clear, autoincrement,
    /// pre-cache refill). `a` is even (word-aligned).
    /// A whole-word VDP port read (recon R1/R3). Returns the value plus the CPU wait cycles the access costs
    /// (a data-port read stalls while the write FIFO drains — recon R3). Status / HV reads never stall.
    fn vdp_read_word(&mut self, a: u32) -> (u16, u32) {
        match a {
            0xC0_0000 | 0xC0_0002 => {
                let open_bus = *self.last_bus_word;
                self.vdp.data_read_at(open_bus, self.now_mclk)
            }
            // Status read: the VDP drives only the low 10 lines — bits 10-15 float with the open-bus
            // residue (K4-5; memtest row 11 `4E88`). Same latch plumbing as the data-port read above.
            0xC0_0004 | 0xC0_0006 => (
                self.vdp
                    .control_read_status(*self.last_bus_word, self.now_mclk),
                0,
            ),
            0xC0_0008..=0xC0_000F => (self.vdp.hv_counter_read(self.now_mclk), 0),
            _ => (*self.last_bus_word, 0),
        }
    }

    /// A whole-word VDP port write (recon R1). `a` is even (word-aligned). Returns the CPU wait cycles the
    /// access costs (recon R3: a data-port write to a full FIFO stalls the 68k via /DTACK) — folded into the
    /// instruction cost through the `Bus68k` wait channel. Control-port writes never stall.
    fn vdp_write_word(&mut self, a: u32, value: u16) -> u32 {
        match a {
            0xC0_0000 | 0xC0_0002 => {
                // A data-port write may also trigger a VRAM fill (recon R4(b)); run any armed DMA after it.
                let wait = self.vdp.data_write_at(value, self.now_mclk);
                wait + self.run_pending_dma()
            }
            0xC0_0004 | 0xC0_0006 => {
                self.vdp.control_write(value, self.now_mclk);
                self.run_pending_dma() // a CD5 Mem/Copy command triggers here (recon R4)
            }
            _ => 0, // HV counter port writes ($C00008–$C0000F) are accepted but not stored.
        }
    }

    /// Execute the DMA the last control/data write armed (recon R4), returning the CPU wait cycles it costs the
    /// 68k. Mode (a) 68k→VDP holds the bus for the whole transfer (a total halt window); fill/copy leave the
    /// 68k running (0 wait). The VDP owns the target write + register bookkeeping; the bus owns the 68k source
    /// reads (it holds ROM/RAM).
    fn run_pending_dma(&mut self) -> u32 {
        let Some(req) = self.vdp.take_dma_request() else {
            return 0;
        };
        match req {
            DmaRequest::Mem { source, len } => self.run_mem_dma(source, len),
            DmaRequest::Fill { len, fill } => {
                // VRAM fill: 68k keeps running (recon R4(b)) — the VDP fills + opens the busy window; 0 wait.
                self.vdp.run_fill(len, fill, self.now_mclk);
                0
            }
            DmaRequest::Copy { source, len } => {
                // VRAM copy: 68k keeps running (recon R4(c)) — the VDP copies + opens the busy window; 0 wait.
                self.vdp.run_copy(source, len, self.now_mclk);
                0
            }
        }
    }

    /// 68k→VDP transfer (recon R4(a)): read `len` words from 68k byte address `source`, feed each through the
    /// FIFO to the current data-port target (SAT write-through fires for VRAM — R5), and hold the 68k bus for
    /// the whole transfer (the total halt window, returned as CPU wait cycles). Advances the source/length
    /// registers to their post-transfer state (recon R4).
    fn run_mem_dma(&mut self, source: u32, len: u16) -> u32 {
        let now = self.now_mclk;
        let count = if len == 0 { 0x1_0000u32 } else { len as u32 };
        let dest = self.vdp.dma_dest();
        let target = self.vdp.dma_target();
        let mut src = source;
        for _ in 0..count {
            let hi = self
                .mapped_byte(src & ADDR_MASK)
                .unwrap_or((*self.last_bus_word >> 8) as u8);
            let lo = self
                .mapped_byte(src.wrapping_add(1) & ADDR_MASK)
                .unwrap_or((*self.last_bus_word & 0xFF) as u8);
            self.vdp.dma_write_word(((hi as u16) << 8) | lo as u16);
            src = src.wrapping_add(2);
        }
        let slots_per_word = if target == Target::Vram { 2 } else { 1 };
        let cost = self.vdp.dma_cost(count as u64 * slots_per_word, now);
        let record = DmaRecord {
            mode: DmaMode::Mem,
            source,
            dest,
            len,
            target,
        };
        self.vdp.dma_complete(record, src >> 1, now + cost);
        cost.div_ceil(MCLK_PER_CPU_CYCLE) as u32
    }
}

impl<'a, S: BusEventSink> Bus68k for MegaDriveBus<'a, S> {
    fn read16(&mut self, addr: u32, fc: u8) -> (u16, u32) {
        let a = addr & ADDR_MASK;
        if fc == 7 {
            // The 68k interrupt-acknowledge (/INTAK) cycle: decode the acknowledged level from the address
            // (`0xFFFFFFF1 | level << 1`) and clear the VDP's pending latch for exactly that level — the ONLY
            // thing that clears the latches (recon R12, docs/2026-07-16-vdp-recon.md). The CPU discards the
            // returned value (Mega Drive VPA → autovector). Zero CPU changes: the interrupt recipe already
            // drives this fc=7 read (microop.rs IntAck), so the deassert rides the existing hook.
            let level = ((a >> 1) & 0x07) as u8;
            self.vdp.acknowledge(level);
            let v = *self.last_bus_word;
            self.emit(BusOp::Read, fc, a, Size::Word, v as u32);
            return (v, 0);
        }
        if Self::is_vdp_port(a) {
            let (value, wait) = self.vdp_read_word(a);
            *self.last_bus_word = value;
            self.emit(BusOp::Read, fc, a, Size::Word, value as u32);
            return (value, wait);
        }
        // K4-3: word-wide access to the Z80 space is impossible — the arbiter runs ONE 8-bit Z80-bus
        // cycle (the even byte) and mirrors the result into both halves (MDBusArbiter.cpp:489-495;
        // memtest row 3: bytes `F3 ED`, word `F3F3`). A closed window falls through to the open-bus arm.
        if (0xA0_0000..=0xA0_FFFF).contains(&a) {
            if let Some(b) = self.mapped_byte(a) {
                let v = (b as u16) * 0x0101;
                *self.last_bus_word = v;
                self.emit(BusOp::Read, fc, a, Size::Word, v as u32);
                return (v, 0);
            }
        }
        let value = if let Some(hi) = self.mapped_byte(a) {
            let lo = self
                .mapped_byte((a.wrapping_add(1)) & ADDR_MASK)
                .unwrap_or(0);
            let v = ((hi as u16) << 8) | lo as u16;
            *self.last_bus_word = v; // a real word crossed the bus
            v
        } else {
            self.open_word(a) // open bus: residue per region flavor (K4-1), latch unchanged
        };
        self.emit(BusOp::Read, fc, a, Size::Word, value as u32);
        (value, 0)
    }

    fn write16(&mut self, addr: u32, fc: u8, value: u16) -> u32 {
        let a = addr & ADDR_MASK;
        if Self::is_vdp_port(a) {
            let wait = self.vdp_write_word(a, value);
            *self.last_bus_word = value;
            self.emit(BusOp::Write, fc, a, Size::Word, value as u32);
            return wait;
        }
        // Q4: word-wide access to the Z80 window is impossible — the arbiter runs ONE 8-bit Z80-bus
        // cycle, so only the HIGH byte lands, at the (even) target address; the low byte is never
        // written. Adjudicated from the reference arbiter (MDBusArbiter.cpp:496-501: even address ->
        // `data.GetUpperHalf()`, one `WriteMemory`), corroborated by Genesis Plus GX (`mem68k.c`
        // `z80_write_word` stores `data >> 8` only) and Plutiedev ("you must use byte accesses when
        // touching Z80 RAM, word accesses won't work"); the read side of the same one-cycle mechanism
        // is hardware-pinned by memtest row 3 (`F3F3`). Exercised by real ROMs: Gunstar Heroes and
        // Alien Soldier clear all 8 KiB of Z80 RAM with a 4096-word sweep (probe column `wwW!`).
        if (0xA0_0000..=0xA0_FFFF).contains(&a) {
            self.store_byte(a, (value >> 8) as u8);
        } else {
            self.store_byte(a, (value >> 8) as u8);
            self.store_byte((a.wrapping_add(1)) & ADDR_MASK, (value & 0xFF) as u8);
        }
        *self.last_bus_word = value;
        self.emit(BusOp::Write, fc, a, Size::Word, value as u32);
        0
    }

    fn read8(&mut self, addr: u32, fc: u8) -> (u8, u32) {
        let a = addr & ADDR_MASK;
        if Self::is_vdp_port(a) {
            // A byte read of a 16-bit VDP port does the stateful word access on the even base and returns
            // the addressed half (even = upper byte, odd = lower).
            let (word, wait) = self.vdp_read_word(a & !1);
            let value = if a & 1 == 0 {
                (word >> 8) as u8
            } else {
                (word & 0xFF) as u8
            };
            *self.last_bus_word = word;
            self.emit(BusOp::Read, fc, a, Size::Byte, value as u32);
            return (value, wait);
        }
        let value = if let Some(b) = self.mapped_byte(a) {
            // A byte read drives only its own half of the data bus; the other half keeps floating —
            // merge the driven lane into the latch (Exodus's tri-state rule, M68000.cpp:2138; K4-1
            // replaces the old both-halves `b * 0x0101` smear).
            *self.last_bus_word = if a & 1 == 0 {
                (*self.last_bus_word & 0x00FF) | ((b as u16) << 8)
            } else {
                (*self.last_bus_word & 0xFF00) | b as u16
            };
            b
        } else if a & 1 == 0 {
            (self.open_word(a) >> 8) as u8 // even address → UDS half
        } else {
            (self.open_word(a) & 0xFF) as u8 // odd address → LDS half
        };
        self.emit(BusOp::Read, fc, a, Size::Byte, value as u32);
        (value, 0)
    }

    fn write8(&mut self, addr: u32, fc: u8, value: u8) -> u32 {
        let a = addr & ADDR_MASK;
        if Self::is_vdp_port(a) {
            // A byte write to a 16-bit VDP port drives the byte on both halves (the common byte-write model);
            // the stateful word write runs on the even base.
            let word = (value as u16) * 0x0101;
            let wait = self.vdp_write_word(a & !1, word);
            *self.last_bus_word = word;
            self.emit(BusOp::Write, fc, a, Size::Byte, value as u32);
            return wait;
        }
        self.store_byte(a, value);
        *self.last_bus_word = (value as u16) * 0x0101;
        self.emit(BusOp::Write, fc, a, Size::Byte, value as u32);
        0
    }

    fn tas(&mut self, addr: u32, fc: u8) -> (u8, u32) {
        // The Mega Drive bus controller does NOT honor the RMW write cycle of TAS (the Gargoyles/Ex-Mutants
        // quirk): the read happens, the write is DROPPED. So we read `orig`, DO NOT store `orig | 0x80`, and
        // still emit the Tas event (its value = the byte the CPU drove for the dropped write). The CPU gets
        // `orig` back for its flags.
        let a = addr & ADDR_MASK;
        let orig = if let Some(b) = self.mapped_byte(a) {
            b
        } else {
            (*self.last_bus_word & 0xFF) as u8
        };
        let written = orig | 0x80;
        *self.last_bus_word = (written as u16) * 0x0101;
        self.emit(BusOp::Tas, fc, a, Size::Byte, written as u32);
        (orig, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SY-4a forwarder-equivalence (design §7 Test 4): a sink that implements ONLY `on_event`
    /// must receive an identical event whether the emitting site calls `on_event` directly or
    /// routes through the defaulted `on_event_at(ev, mclk)` — for any mclk. This pins the default
    /// forwarder so the two real emission sites can switch to the timestamped path with zero
    /// behavioral change to `()`/`Vec<BusEvent>`/Watchpoints/VgmLogger.
    #[test]
    fn on_event_at_default_forwards_identically_for_any_mclk() {
        let ev = BusEvent {
            op: BusOp::Write,
            fc: 0,
            addr: 0xA0_4000,
            size: Size::Byte,
            value: 0x2A,
        };
        for &mclk in &[0u64, 1, 1219, MD_VERSION as u64, 896_040, u64::MAX] {
            // Path A: direct untimed delivery.
            let mut direct: Vec<BusEvent> = Vec::new();
            direct.on_event(ev);
            // Path B: timestamped delivery through the default forwarder (no override on Vec).
            let mut timed: Vec<BusEvent> = Vec::new();
            timed.on_event_at(ev, mclk);
            assert_eq!(
                direct, timed,
                "default on_event_at must deliver exactly the same event (mclk = {mclk})"
            );
            assert_eq!(timed, vec![ev]);
        }
    }

    #[test]
    fn read_returns_ram_byte_and_emits_event() {
        let mut ram = vec![0u8; RAM_SIZE];
        let mut vram = vec![0u8; VRAM_SIZE];
        ram[5] = 0x7E;
        let mut sink: Vec<BusEvent> = Vec::new();
        let mut bus = SystemBus::new(&mut ram, &mut vram, &mut sink);
        let v = bus.read(RAM_BASE + 5, Size::Byte);
        drop(bus);
        assert_eq!(v, 0x7E);
        assert_eq!(
            sink,
            vec![BusEvent {
                op: BusOp::Read,
                fc: 0,
                addr: RAM_BASE + 5,
                size: Size::Byte,
                value: 0x7E,
            }]
        );
    }

    #[test]
    fn word_read_is_big_endian() {
        let mut ram = vec![0u8; RAM_SIZE];
        let mut vram = vec![0u8; VRAM_SIZE];
        ram[0] = 0x12;
        ram[1] = 0x34;
        let mut sink: Vec<BusEvent> = Vec::new();
        let mut bus = SystemBus::new(&mut ram, &mut vram, &mut sink);
        let v = bus.read(RAM_BASE, Size::Word);
        drop(bus);
        assert_eq!(v, 0x1234);
    }

    #[test]
    fn ram_write_is_immediate() {
        let mut ram = vec![0u8; RAM_SIZE];
        let mut vram = vec![0u8; VRAM_SIZE];
        let mut sink: Vec<BusEvent> = Vec::new();
        let mut bus = SystemBus::new(&mut ram, &mut vram, &mut sink);
        bus.write(RAM_BASE + 3, Size::Byte, 0xAB);
        let readback = bus.read(RAM_BASE + 3, Size::Byte);
        drop(bus);
        assert_eq!(readback, 0xAB);
        assert_eq!(ram[3], 0xAB);
    }

    #[test]
    fn word_write_is_big_endian() {
        let mut ram = vec![0u8; RAM_SIZE];
        let mut vram = vec![0u8; VRAM_SIZE];
        let mut sink: Vec<BusEvent> = Vec::new();
        let mut bus = SystemBus::new(&mut ram, &mut vram, &mut sink);
        bus.write(RAM_BASE + 10, Size::Word, 0xBEEF);
        drop(bus);
        assert_eq!(ram[10], 0xBE);
        assert_eq!(ram[11], 0xEF);
    }

    #[test]
    fn vram_write_is_deferred_until_apply() {
        let mut ram = vec![0u8; RAM_SIZE];
        let mut vram = vec![0u8; VRAM_SIZE];
        let mut sink: Vec<BusEvent> = Vec::new();
        let mut bus = SystemBus::new(&mut ram, &mut vram, &mut sink);
        let before = bus.read(VRAM_BASE, Size::Byte);
        bus.write(VRAM_BASE, Size::Byte, 0xCD);
        let mid = bus.read(VRAM_BASE, Size::Byte);
        bus.apply_writes();
        let after = bus.read(VRAM_BASE, Size::Byte);
        drop(bus);
        assert_eq!((before, mid, after), (0, 0, 0xCD));
        assert_eq!(vram[0], 0xCD);
    }

    struct WriteWatch {
        target: u32,
        hits: u32,
    }
    impl BusEventSink for WriteWatch {
        fn on_event(&mut self, event: BusEvent) {
            if event.op == BusOp::Write && event.addr == self.target {
                self.hits += 1;
            }
        }
    }

    #[test]
    fn system_bus_events_carry_fc_zero_for_a_non_cpu_master() {
        // SystemBus is not a CPU master — it has no function code, so every event it emits reports fc = 0.
        // (The CPU-side MegaDriveBus adapter fills the real 68000 FC; DMA and later chips also emit 0.)
        let mut ram = vec![0u8; RAM_SIZE];
        let mut vram = vec![0u8; VRAM_SIZE];
        let mut sink: Vec<BusEvent> = Vec::new();
        let mut bus = SystemBus::new(&mut ram, &mut vram, &mut sink);
        bus.read(RAM_BASE, Size::Byte);
        bus.write(RAM_BASE, Size::Byte, 1);
        drop(bus);
        assert!(sink.iter().all(|e| e.fc == 0), "SystemBus emits fc = 0");
        // BusOp::Tas is a distinct third kind (the indivisible RMW), not a Read or a Write.
        assert_ne!(BusOp::Tas, BusOp::Read);
        assert_ne!(BusOp::Tas, BusOp::Write);
    }

    #[test]
    fn instrumentation_is_an_event_consumer() {
        let mut ram = vec![0u8; RAM_SIZE];
        let mut vram = vec![0u8; VRAM_SIZE];
        let mut watch = WriteWatch {
            target: RAM_BASE + 0x20,
            hits: 0,
        };
        let mut bus = SystemBus::new(&mut ram, &mut vram, &mut watch);
        bus.write(RAM_BASE + 0x20, Size::Byte, 1);
        bus.read(RAM_BASE + 0x20, Size::Byte);
        bus.write(RAM_BASE + 0x20, Size::Byte, 2);
        bus.write(RAM_BASE + 0x21, Size::Byte, 3);
        drop(bus);
        assert_eq!(watch.hits, 2);
    }

    // --- MegaDriveBus: the real memory map ---------------------------------------------------------------

    use crate::m68000::bus68k::Bus68k;

    /// Backing store for a MegaDriveBus under test: a ROM, 64 KiB work RAM, 8 KiB Z80 RAM, a VDP, the
    /// master-clock reading, the open-bus latch.
    struct MdMem {
        rom: Vec<u8>,
        ram: Vec<u8>,
        z80_ram: Vec<u8>,
        vdp: Vdp,
        io: Io,
        now_mclk: u64,
        last_bus_word: u16,
        z80_busreq: bool,
        z80_running: bool,
        z80_bank: u16,
        sram_enabled: bool,
        sram_write_protect: bool,
        sram: Vec<u8>,
        sram_dirty: bool,
        sram_used: bool,
        sram_map: Option<SramMap>,
        fm: Ym2612,
    }
    impl MdMem {
        fn new(rom: Vec<u8>) -> Self {
            Self {
                rom,
                ram: vec![0u8; RAM_SIZE],
                z80_ram: vec![0u8; Z80_RAM_SIZE],
                vdp: Vdp::power_on(&mut crate::rng::SplitMix64::new(1)),
                io: Io::default(),
                now_mclk: 0,
                last_bus_word: 0,
                z80_busreq: false,
                z80_running: false,
                z80_bank: 0,
                sram_enabled: false,
                sram_write_protect: false,
                sram: Vec::new(),
                sram_dirty: false,
                sram_used: false,
                sram_map: None,
                fm: Ym2612::new(),
            }
        }
        fn bus<'a>(&'a mut self, sink: &'a mut Vec<BusEvent>) -> MegaDriveBus<'a, Vec<BusEvent>> {
            MegaDriveBus::new(
                &self.rom,
                &mut self.ram,
                &mut self.z80_ram,
                &mut self.vdp,
                &mut self.io,
                self.now_mclk,
                &mut self.last_bus_word,
                &mut self.z80_busreq,
                &mut self.z80_running,
                &mut self.z80_bank,
                &mut self.sram_enabled,
                &mut self.sram_write_protect,
                &mut self.sram,
                &mut self.sram_dirty,
                &mut self.sram_used,
                self.sram_map,
                &mut self.fm,
                sink,
            )
        }
    }

    #[test]
    fn rom_read_returns_the_rom_byte() {
        let mut rom = vec![0u8; 0x1000];
        rom[0] = 0x12;
        rom[1] = 0x34;
        rom[0x10] = 0xAB;
        let mut mem = MdMem::new(rom);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        assert_eq!(bus.read16(0x00_0000, 6).0, 0x1234, "word from ROM");
        assert_eq!(bus.read8(0x00_0010, 6).0, 0xAB, "byte from ROM");
    }

    #[test]
    fn rom_is_read_only_a_write_does_not_change_it() {
        let mut mem = MdMem::new(vec![0x11u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        bus.write16(0x00_0000, 6, 0xFFFF);
        assert_eq!(
            bus.read16(0x00_0000, 6).0,
            0x1111,
            "ROM unchanged by a write"
        );
    }

    #[test]
    fn rom_past_the_end_is_open_bus() {
        // A short 4 KiB ROM: reads past its end return the open-bus latch, NOT a mirror of the ROM.
        let mut mem = MdMem::new(vec![0x11u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        // Drive a known word onto the bus (a work-RAM write), then read past the ROM end.
        bus.write16(0xE0_0000, 5, 0xBEEF);
        assert_eq!(
            bus.read16(0x20_0000, 6).0,
            0xBEEF,
            "past-end ROM read returns the last bus word"
        );
    }

    #[test]
    fn unmapped_range_is_open_bus_and_does_not_change_the_latch() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        // K4-1: $400000-$7FFFFF is ARBITER-flavored open bus — high-byte residue, low byte $00.
        bus.write16(0xE0_0000, 5, 0xCAFE); // latch := 0xCAFE
        assert_eq!(
            bus.read16(0x50_0000, 6).0,
            0xCA00,
            "unmapped read = residue high byte | $00"
        );
        // A second open-bus read still sees the same latch (an open-bus read does not drive a new word).
        assert_eq!(bus.read16(0x60_0000, 6).0, 0xCA00);
    }

    #[test]
    fn work_ram_reads_writes_and_mirrors_across_the_window() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        bus.write16(0xE0_0000, 5, 0x1357);
        assert_eq!(bus.read16(0xE0_0000, 5).0, 0x1357, "written then read back");
        // The 64 KiB work RAM is mirrored across the whole $E00000–$FFFFFF window.
        assert_eq!(bus.read16(0xFF_0000, 5).0, 0x1357, "mirror at $FF0000");
        assert_eq!(bus.read16(0xF1_0000, 5).0, 0x1357, "mirror at $F10000");
    }

    /// Open the 68k-side Z80 window the way real init code does (K4-3 gate): release Z80 reset, then
    /// assert BUSREQ — the window forwards only while `z80_busreq && z80_running`.
    fn open_z80_window(bus: &mut MegaDriveBus<'_, Vec<BusEvent>>) {
        bus.write16(0xA1_1200, 5, 0x0100); // release reset
        bus.write16(0xA1_1100, 5, 0x0100); // request the Z80 bus
    }

    #[test]
    fn z80_ram_reads_writes_and_mirrors_in_its_window() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        open_z80_window(&mut bus); // K4-3: the window forwards only while busreq is held + reset released
        bus.write8(0xA0_0000, 5, 0x9A);
        assert_eq!(bus.read8(0xA0_0000, 5).0, 0x9A, "Z80 RAM byte round-trips");
        // 8 KiB Z80 RAM mirrored across the 64 KiB window.
        assert_eq!(bus.read8(0xA0_2000, 5).0, 0x9A, "mirror at +0x2000");
    }

    #[test]
    fn z80_window_closed_is_arbiter_open_bus_and_drops_writes() {
        // K4-3 (design §3 rows 2/3): the 68k-side Z80 window forwards only when the 68k owns the Z80 bus
        // AND reset is released (MDBusArbiter.cpp:482 `!reset && busgrant`). Closed — power-on, or BUSREQ
        // held while reset is asserted (memtest row 2: `4E00 4E00`) — reads are arbiter open bus and
        // writes are dropped.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.z80_ram[0] = 0xF3;
        let mut sink = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);

            // Power-on: both latches off -> closed.
            bus.write16(0xE0_0000, 5, 0x4E71); // residue
            assert_eq!(bus.read16(0xA0_0000, 5).0, 0x4E00, "closed: word = 4E00");
            bus.write16(0xE0_0000, 5, 0x4E71);
            assert_eq!(
                bus.read8(0xA0_0000, 5).0,
                0x4E,
                "closed: even byte = residue hi"
            );
            assert_eq!(bus.read8(0xA0_0001, 5).0, 0x00, "closed: odd byte = $00");

            // Row 2's exact condition: BUSREQ held, reset still asserted -> STILL closed.
            bus.write16(0xA1_1100, 5, 0x0100);
            bus.write16(0xE0_0000, 5, 0x4E71);
            assert_eq!(
                bus.read16(0xA0_0000, 5).0,
                0x4E00,
                "busreq alone does not open the window while reset is asserted"
            );

            // A write through the closed window is dropped.
            bus.write8(0xA0_0000, 5, 0xAB);
        }
        assert_eq!(mem.z80_ram[0], 0xF3, "closed-window write dropped");
    }

    #[test]
    fn z80_window_word_read_duplicates_the_even_byte() {
        // K4-3 (design §3 row 3): word-wide access to the Z80 space is impossible — the arbiter runs ONE
        // 8-bit Z80-bus cycle and mirrors the result into both halves (MDBusArbiter.cpp:489-495). memtest:
        // bytes read `F3 ED`, the word reads `F3F3`.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.z80_ram[0] = 0xF3;
        mem.z80_ram[1] = 0xED;
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        open_z80_window(&mut bus);
        assert_eq!(bus.read8(0xA0_0000, 5).0, 0xF3, "even byte");
        assert_eq!(bus.read8(0xA0_0001, 5).0, 0xED, "odd byte");
        assert_eq!(
            bus.read16(0xA0_0000, 5).0,
            0xF3F3,
            "word = the even byte mirrored into both halves"
        );
    }

    #[test]
    fn z80_bank_and_unused_region_reads_ff_through_the_open_window() {
        // K4-3 (design §3 row 5): `$A06000-$A07EFF` (the Z80-side bank register / unused region) reads
        // `$FF` through the open window — the reference arbiter returns all-ones there ("hardware tests",
        // MDBusArbiter.cpp:422-437); memtest: `FFFF FFFF`. NOT the Z80-RAM mirror.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.z80_ram[0] = 0xF3; // $A06000 & $1FFF = 0 — would alias this under the old mirror
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        open_z80_window(&mut bus);
        assert_eq!(bus.read8(0xA0_6000, 5).0, 0xFF, "bank register byte = $FF");
        assert_eq!(bus.read8(0xA0_7EFF, 5).0, 0xFF, "top of the region = $FF");
        assert_eq!(bus.read16(0xA0_6000, 5).0, 0xFFFF, "word = $FFFF");
    }

    #[test]
    fn z80_bank_region_writes_do_not_alias_into_z80_ram() {
        // K4-3 rider: $6000-$7FFF on the Z80 side is the bank register / unused / port-mirror region —
        // NOT RAM. Before this fix a 68k write to $A06000 (memtest writes a $FF bank canary at f11)
        // landed in z80_ram[0] through the `& $1FFF` mirror, corrupting the loaded Z80 program and
        // breaking the row-3 reference (`F3ED` read back `FFED`). The write must drop from RAM's view.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.z80_ram[0] = 0xF3;
        let mut sink = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);
            open_z80_window(&mut bus);
            bus.write8(0xA0_6000, 5, 0xFF);
        }
        assert_eq!(
            mem.z80_ram[0], 0xF3,
            "a bank-region write must not corrupt Z80 RAM through the mirror"
        );
    }

    #[test]
    fn z80_window_is_masked_to_15_bits() {
        // The window mirrors the Z80's 15-bit local bus (MDBusArbiter.cpp:304, Charles MacDonald's
        // hardware tests): $A08000-$A0FFFF behaves exactly as $A00000-$A07FFF. So +$8000 aliases RAM,
        // $A0C000 reaches the FM ports (offset $4000), and $A0E000 is bank-register territory ($FF) —
        // NOT the old 8-KiB RAM smear.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.z80_ram[5] = 0x77;
        mem.z80_ram[0] = 0xF3;
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        open_z80_window(&mut bus);
        assert_eq!(bus.read8(0xA0_8005, 5).0, 0x77, "+$8000 mirrors Z80 RAM");
        bus.write8(0xA0_8005, 5, 0x88);
        assert_eq!(bus.read8(0xA0_0005, 5).0, 0x88, "+$8000 write lands in RAM");
        assert_eq!(
            bus.read8(0xA0_C000, 5).0 & 0x80,
            0,
            "$A0C000 = FM status (offset $4000), BUSY clear — not z80_ram[0]=$F3"
        );
        assert_ne!(bus.read8(0xA0_C000, 5).0, 0xF3, "not the RAM byte");
        assert_eq!(
            bus.read8(0xA0_E000, 5).0,
            0xFF,
            "$A0E000 = bank-register territory (offset $6000): $FF"
        );
    }

    #[test]
    fn fm_answers_across_its_full_select_span() {
        // memtest row `A04000-A05FFF : 0000 0000` — the YM2612's chip select spans the whole $4000-$5FFF
        // window offset (ports = the low 2 bits), so $A04004+ reads the status byte too, NOT the old
        // Z80-RAM mirror.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.z80_ram[0x0004] = 0xAB; // would alias $A04004 under the old & $1FFF smear
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        open_z80_window(&mut bus);
        for a in [0xA0_4004u32, 0xA0_4100, 0xA0_5FFF] {
            assert_eq!(bus.read8(a, 5).0, 0x00, "FM status at {a:#08X}");
        }
        assert_eq!(
            bus.read16(0xA0_4004, 5).0,
            0x0000,
            "word = status duplicated"
        );
    }

    #[test]
    fn z80_bank_latch_ticks_from_the_68k_side() {
        // The 68k-side path to the bank register: a write into the open window at Z80 offset
        // $6000-$60FF is one serial tick of the SAME 9-bit latch the Z80's own $6000 write loads
        // (Oracle `Z80WindowBankswitch` — one register, two paths). Load 0x101 LSB-first, exactly like
        // the z80/bus.rs test.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);
            open_z80_window(&mut bus);
            for bit in [1u8, 0, 0, 0, 0, 0, 0, 0, 1] {
                bus.write8(0xA0_6000, 5, bit);
            }
        }
        assert_eq!(
            mem.z80_bank, 0x101,
            "9 LSB-first 68k writes select the page"
        );

        // Closed window: the arbiter drops the write — the latch must NOT tick.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);
            bus.write8(0xA0_6000, 5, 1);
        }
        assert_eq!(mem.z80_bank, 0, "closed-window bank write drops");
    }

    #[test]
    fn z80_window_word_write_lands_only_the_high_byte() {
        // Q4: word-wide access to the Z80 space is impossible — the arbiter runs ONE 8-bit cycle, so a
        // 68k word write lands only the HIGH byte at the (even) target address; the odd byte is never
        // written (MDBusArbiter.cpp:496-501; Genesis Plus GX `z80_write_word`; Plutiedev "word accesses
        // won't work"). Exercised for real by Gunstar Heroes / Alien Soldier word-wide RAM clears.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.z80_ram[0] = 0x11;
        mem.z80_ram[1] = 0x55;
        let mut sink = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);
            open_z80_window(&mut bus);
            bus.write16(0xA0_0000, 5, 0xABCD);
        }
        assert_eq!(mem.z80_ram[0], 0xAB, "high byte lands at the even address");
        assert_eq!(mem.z80_ram[1], 0x55, "low byte is NEVER written");
    }

    #[test]
    fn vdp_mirror_reads_through_the_68k_window() {
        // K2's 68k-side half: an open-window read at Z80 offset $7F00-$7FFF routes through the SAME
        // `vdp_mirror_read` the Z80's own bus uses — live status bytes at $7F04-$7F07 (same byte-lane
        // split, same `open_bus = 0` pin), live HV at $7F08-$7F0F, and the data-port mirror $7F00-$7F03
        // stays `$FF` (the ledgered `vdp-dataport-read-lockup` known-difference).
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let expected_status = mem.vdp.status_word(0);
        let expected_hv = mem.vdp.hv_counter_read(0);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        open_z80_window(&mut bus);
        assert_eq!(
            bus.read8(0xA0_7F04, 5).0,
            (expected_status >> 8) as u8,
            "even byte = status high"
        );
        assert_eq!(
            bus.read8(0xA0_7F05, 5).0,
            (expected_status & 0xFF) as u8,
            "odd byte = status low"
        );
        assert_eq!(
            bus.read8(0xA0_7F08, 5).0,
            (expected_hv >> 8) as u8,
            "even byte = V counter"
        );
        assert_eq!(bus.read8(0xA0_7F00, 5).0, 0xFF, "data-port mirror = $FF");
        // And through the +$8000 mirror (15-bit masking).
        assert_eq!(
            bus.read8(0xA0_FF05, 5).0,
            (expected_status & 0xFF) as u8,
            "same through the +$8000 mirror"
        );
    }

    #[test]
    fn psg_write_through_the_68k_window_taps_the_z80_shaped_event() {
        // The window's $7F11 (PSG through the VDP-port mirror) taps the SAME BusEvent shape the Z80's
        // own $7F11 write emits (addr $7F11, fc 0, byte) — so the VGM logger/synth unify both paths.
        // The bus's own $A07F11 write event still follows (every 68k write emits).
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);
            open_z80_window(&mut bus);
            bus.write8(0xA0_7F11, 5, 0x9F);
        }
        let tap = BusEvent {
            op: BusOp::Write,
            fc: 0,
            addr: 0x7F11,
            size: Size::Byte,
            value: 0x9F,
        };
        assert!(
            sink.contains(&tap),
            "the Z80-shaped PSG tap event is emitted: {sink:?}"
        );
        // Closed window: no tap.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);
            bus.write8(0xA0_7F11, 5, 0x9F);
        }
        assert!(
            !sink.iter().any(|e| e.addr == 0x7F11),
            "closed-window PSG write drops without a tap"
        );
    }

    #[test]
    fn version_register_returns_the_fixed_constant() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        assert_eq!(bus.read8(0xA1_0001, 5).0, MD_VERSION, "version register");
    }

    #[test]
    fn io_registers_ignore_a0_even_byte_and_word_reads_mirror_the_register() {
        // K4-4 (design §3 row 6): the I/O block does not decode A0 (Exodus
        // `AddressDiscardLowerBitCount="1"`), so each register answers BOTH byte lanes: the even byte
        // reads the same register as its odd neighbour, and a word read is the register duplicated —
        // memtest reads `A0A0 A0A0` at $A10000 (the version byte), not `00A0`.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        assert_eq!(bus.read8(0xA1_0000, 5).0, MD_VERSION, "even byte = version");
        assert_eq!(
            bus.read16(0xA1_0000, 5).0,
            (MD_VERSION as u16) * 0x0101,
            "word = version duplicated (A0A0)"
        );
        // A configured register mirrors too: P1 ctrl = $40.
        bus.write8(0xA1_0009, 5, 0x40);
        assert_eq!(bus.read8(0xA1_0008, 5).0, 0x40, "even byte = P1 ctrl");
        assert_eq!(bus.read16(0xA1_0008, 5).0, 0x4040, "word = ctrl duplicated");
        // RxData still reads 0 on both lanes (no serial device drives the line).
        assert_eq!(bus.read16(0xA1_0010, 5).0, 0x0000, "RxData word = 0000");
    }

    #[test]
    fn port1_pad_reads_through_the_th_protocol_over_the_bus() {
        // Drive the whole read through the CPU-facing read8/write8 path — never Io directly. Inject Start on
        // P1, configure TH as output ($40), and read both nibbles (recon IO4). The version byte still holds.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.io.set_pad(
            0,
            crate::io::Pad {
                start: true,
                ..Default::default()
            },
        );
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        // P1 control: TH output.
        bus.write8(0xA1_0009, 5, 0x40);
        // TH=1: Start is on the TH=0 nibble, so it must NOT appear here — the C/B/R/L/D/U set reads released.
        bus.write8(0xA1_0003, 5, 0x40);
        assert_eq!(
            bus.read8(0xA1_0003, 5).0,
            0xFF,
            "TH=1: nothing pressed in the C/B/R/L/D/U set"
        );
        // TH=0: bit 5 (Start) reads low; bits 3,2 forced low (detection signature); read = 0x93.
        bus.write8(0xA1_0003, 5, 0x00);
        let lo = bus.read8(0xA1_0003, 5).0;
        assert_eq!(lo & 0x20, 0, "Start pressed (bit 5 = 0)");
        assert_eq!(
            lo & 0x0C,
            0,
            "bits 3,2 forced low (MD-pad detection signature)"
        );
        assert_eq!(lo, 0x93);
        // The version register is untouched by the wiring.
        assert_eq!(
            bus.read8(0xA1_0001, 5).0,
            MD_VERSION,
            "version register still fixed"
        );
    }

    #[test]
    fn z80_busreq_reflects_the_request_latch() {
        // `$A11100` bit0: 0 = 68000 granted the bus (BUSREQ asserted), 1 = Z80 owns it (released). Two real-
        // game idioms depend on this: take-bus (assert, wait for bit0 -> 0) and release (deassert, wait for
        // bit0 -> 1). The old constant-0 stub satisfied take-bus but hung the release spin forever (DR-1
        // Gunstar). Semantics + in-situ evidence: docs/2026-07-22-z80-busreq-recon.md (Z2, Z5).
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);

        // Power-on: nothing has requested the bus, so the Z80 owns it (bit0 = 1).
        assert_eq!(
            bus.read8(0xA1_1100, 5).0 & 1,
            1,
            "power-on -> Z80 owns the bus (bit0 = 1)"
        );

        // Release Z80 reset first — since K4-2 the readable bit folds RESET in (hardware/Exodus:
        // 1 = "bus unavailable" while reset is asserted, regardless of BUSREQ), which is exactly what
        // real init code does before spinning on the grant (memtest, ristar, gunstar).
        bus.write16(0xA1_1200, 5, 0x0100);

        // Assert via the word idiom `move.w #$100,$A11100` — byte $01 lands at the even address, $00 at the
        // odd one; the odd byte must NOT clobber the latch.
        bus.write16(0xA1_1100, 5, 0x0100);
        assert_eq!(
            bus.read8(0xA1_1100, 5).0 & 1,
            0,
            "asserted -> granted to 68000 (bit0 = 0)"
        );

        // Release via `move.w #$0,$A11100`: bit0 returns to 1 (Z80 owns the bus). The old stub hung here.
        bus.write16(0xA1_1100, 5, 0x0000);
        assert_eq!(
            bus.read8(0xA1_1100, 5).0 & 1,
            1,
            "released -> Z80 owns the bus (bit0 = 1)"
        );

        // The byte-write idiom asserts too.
        bus.write8(0xA1_1100, 5, 0x01);
        assert_eq!(
            bus.read8(0xA1_1100, 5).0 & 1,
            0,
            "byte assert -> granted (bit0 = 0)"
        );
    }

    #[test]
    fn a11100_reads_residue_in_bits_9_15_low_byte_00_and_folds_reset_into_the_grant_bit() {
        // K4-2 (design §3 rows 7/8): `$A11100` is partially decoded — the arbiter drives ONLY the grant
        // bit (bit 8 of the word / bit 0 of the even byte, MDBusArbiter.cpp:442-447 + the XML data-line
        // mapping) and the low byte to $00; bits 9-15 float with the residue. The readable bit folds in
        // Z80 RESET: 1 = "bus unavailable" = !(busreq && reset released) — the memtest column reads
        // `4F00` with BUSREQ held while reset is asserted (row 7), `4E00` once released (row 8).
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);

        // BUSREQ held, reset still asserted: bit reads 1 (unavailable), residue rides bits 9-15.
        bus.write16(0xA1_1100, 5, 0x0100); // assert BUSREQ
        bus.write16(0xE0_0000, 5, 0x4E71); // drive a known residue word
        assert_eq!(
            bus.read16(0xA1_1100, 5).0,
            0x4F00,
            "row 7: residue & $FE00 | grant-unavailable bit | low byte $00"
        );
        bus.write16(0xE0_0000, 5, 0x4E71);
        assert_eq!(bus.read8(0xA1_1100, 5).0, 0x4F, "even byte: residue | bit0");
        assert_eq!(bus.read8(0xA1_1101, 5).0, 0x00, "odd byte driven $00");

        // Release reset (BUSREQ still held): the bus is now genuinely granted -> bit 0.
        bus.write16(0xA1_1200, 5, 0x0100);
        bus.write16(0xE0_0000, 5, 0x4E71);
        assert_eq!(
            bus.read16(0xA1_1100, 5).0,
            0x4E00,
            "row 8: granted, bit = 0"
        );

        // A residue with bit 9 set shows through (the arbiter masks only its own driven lines).
        bus.write16(0xE0_0000, 5, 0xABCD);
        assert_eq!(
            bus.read16(0xA1_1100, 5).0,
            0xAA00,
            "residue $ABCD -> $AA00 (bit 8 cleared by grant, bit 0 region $00)"
        );
    }

    #[test]
    fn z80_reset_latch_powers_on_asserted_and_toggles() {
        // `$A11200` bit0 is the Z80 reset-release latch (`z80_running`): power-on = reset ASSERTED (Z80
        // held), a write of bit0 = 1 releases it (Z80 runs), a write of bit0 = 0 re-asserts it (design
        // ZC6/ZC13). Since K4-1 the register is WRITE-ONLY (reads are undriven arbiter open bus — the
        // memtest row-9 pin), so the latch is asserted directly on `mem.z80_running`.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);

        // Power-on: reset asserted, Z80 held.
        assert!(!mem.z80_running, "power-on -> reset asserted");

        // Release reset via the word idiom `move.w #$100,$A11200`: $01 lands at the even address, $00 at the
        // odd one; the odd byte must NOT clobber the latch.
        let mut sink = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);
            bus.write16(0xA1_1200, 5, 0x0100);
        }
        assert!(mem.z80_running, "reset released -> Z80 runs");

        // Re-assert via `move.w #$0,$A11200`: held again.
        {
            let mut bus = mem.bus(&mut sink);
            bus.write16(0xA1_1200, 5, 0x0000);
        }
        assert!(!mem.z80_running, "reset re-asserted -> Z80 held");

        // The byte-write idiom releases too.
        {
            let mut bus = mem.bus(&mut sink);
            bus.write8(0xA1_1200, 5, 0x01);
        }
        assert!(mem.z80_running, "byte release -> Z80 runs");

        // A write to the odd half drops — it must not clobber the latch.
        {
            let mut bus = mem.bus(&mut sink);
            bus.write8(0xA1_1201, 5, 0x00);
        }
        assert!(mem.z80_running, "odd-half write drops, latch intact");
    }

    #[test]
    fn fm_status_reads_not_busy_and_writes_do_not_alias_z80_ram() {
        // $A04000-3 = the YM2612 FM chip (recon docs/2026-07-22-fm-status-recon.md F1/F2). Reads return the
        // status byte with bit7 (BUSY) clear, so the `btst #7,(a0)/bne` busy-poll exits (DR-1b Gunstar).
        // Writes go to the FM chip (dropped), NOT to z80_ram: $A04000-3 alias z80_ram[0..3] under the 8 KiB
        // mirror, so before this carve-out an FM register-address write corrupted real Z80 RAM and left the
        // "status" reading busy forever.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        // K4-3: open the window for the Z80-RAM probes below (the FM ports themselves stay ungated —
        // this carve-out precedes the window arm, DR-1b Option B).
        open_z80_window(&mut bus);

        // All four FM ports read not-busy (bit7 clear).
        for a in [0xA0_4000u32, 0xA0_4001, 0xA0_4002, 0xA0_4003] {
            assert_eq!(
                bus.read8(a, 5).0 & 0x80,
                0,
                "FM status bit7 (BUSY) clear at {a:#08X}"
            );
        }

        // An FM register-address write (>= $80, bit7 set) must NOT appear as real Z80 RAM at $A00000 (they
        // alias z80_ram[0]). Old behavior: $A00000 read back $F3 and the busy-poll hung.
        bus.write8(0xA0_4000, 5, 0xF3);
        assert_ne!(
            bus.read8(0xA0_0000, 5).0,
            0xF3,
            "FM write must not corrupt real Z80 RAM at $A00000"
        );
        assert_eq!(
            bus.read8(0xA0_4000, 5).0 & 0x80,
            0,
            "FM status still not-busy after an FM write"
        );

        // Real Z80 RAM at $A00000 still round-trips — the FM carve-out spans window offset $4000-$5FFF
        // (K4-6), never the RAM region.
        bus.write8(0xA0_0000, 5, 0xAB);
        assert_eq!(
            bus.read8(0xA0_0000, 5).0,
            0xAB,
            "Z80 RAM $A00000 still writable after the FM carve-out"
        );
    }

    #[test]
    fn vdp_status_read_returns_the_live_status_word() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        // Line 100 (active), H=0x40 (dot 1280) — not v/h blank, and the FIFO is empty, so bit 9 is the only
        // bit set (A1 made that bit live from `fifo_len` rather than a hardcoded placeholder).
        mem.now_mclk = 100 * crate::vdp::MCLK_PER_LINE + 1280;
        mem.vdp.control_write(0x8140, 0); // display on (bit 3 is forced set while the display is disabled)
        let expected = mem.vdp.status_word(mem.now_mclk);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        // Power-on latch is 0 → the floating upper 6 bits read 0 (K4-5).
        assert_eq!(bus.read16(0xC0_0004, 5).0, expected, "live VDP status word");
        assert_eq!(expected, 0x0200, "FIFO-empty only during active display");
    }

    #[test]
    fn vdp_status_upper_six_bits_carry_the_bus_residue_through_the_port() {
        // K4-5 end-to-end (memtest row 11): drive residue $4E71 onto the bus, read $C00004 — the upper
        // 6 bits are the residue's ($4C00), the low 10 are the live status. The read then latches the
        // merged word (it really crossed the bus), so a SECOND immediate read floats $0000 upper bits
        // only if the residue changed — on real code a prefetch re-drives the latch first.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.now_mclk = 100 * crate::vdp::MCLK_PER_LINE + 1280; // active display: status = $0200
        mem.vdp.control_write(0x8140, 0); // display on (bit 3 is forced set while the display is disabled)
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        bus.write16(0xE0_0000, 5, 0x4E71); // residue
        assert_eq!(
            bus.read16(0xC0_0004, 5).0,
            0x4E00,
            "status = (residue & $FC00) | (live status & $03FF)"
        );
        // Byte lanes split the same merged word: even = residue-carrying high byte.
        bus.write16(0xE0_0000, 5, 0x4E71);
        assert_eq!(bus.read8(0xC0_0004, 5).0, 0x4E, "even byte carries residue");
        assert_eq!(
            bus.read8(0xC0_0005, 5).0,
            0x00,
            "odd byte is the low status"
        );
    }

    #[test]
    fn vdp_data_port_write_then_readback_through_the_bus() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        // Register write $8F02: reg 15 (autoincrement) = 2, so the address advances by a word per access.
        bus.write16(0xC0_0004, 5, 0x8F02);
        // Set up a VRAM write at address 0x0100 (code 0x01): word 1 = 01_00 0001 0000 0000 = 0x4100,
        // word 2 = 0x0000 (CD5-CD2 = 0, A15-A14 = 0).
        bus.write16(0xC0_0004, 5, 0x4100);
        bus.write16(0xC0_0004, 5, 0x0000);
        bus.write16(0xC0_0000, 5, 0xBEEF); // data write → VRAM[0x0100..0x0101], addr → 0x0102
        bus.write16(0xC0_0000, 5, 0xCAFE); // → VRAM[0x0102..0x0103]
                                           // Set up a VRAM read at 0x0100 (code 0x00): word 1 = 0x0100, word 2 = 0x0000.
        bus.write16(0xC0_0004, 5, 0x0100);
        bus.write16(0xC0_0004, 5, 0x0000);
        assert_eq!(
            bus.read16(0xC0_0000, 5).0,
            0xBEEF,
            "first VRAM word reads back"
        );
        assert_eq!(
            bus.read16(0xC0_0000, 5).0,
            0xCAFE,
            "second VRAM word reads back"
        );
    }

    #[test]
    fn vdp_hv_counter_read_returns_the_live_counter() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.now_mclk = 50 * crate::vdp::MCLK_PER_LINE + 800; // a mid-frame instant
        let expected = mem.vdp.hv_counter_read(mem.now_mclk);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        assert_eq!(
            bus.read16(0xC0_0008, 5).0,
            expected,
            "HV counter word ((V<<8)|H) at $C00008"
        );
        // The HV counter is mirrored across $C0000A/$C0000C/$C0000E.
        assert_eq!(
            bus.read16(0xC0_000A, 5).0,
            expected,
            "HV counter mirror at $C0000A"
        );
    }

    /// Drive display-on + autoinc-2 + a VRAM-write command @ `$0100` into the VDP through the control port.
    fn vdp_setup_vram_write(bus: &mut MegaDriveBus<'_, Vec<BusEvent>>) {
        bus.write16(0xC0_0004, 5, 0x8140); // reg 1 = display enable (bit 6)
        bus.write16(0xC0_0004, 5, 0x8F02); // reg 15 = autoinc 2
        bus.write16(0xC0_0004, 5, 0x4100); // VRAM write command, word 1 (@ $0100)
        bus.write16(0xC0_0004, 5, 0x0000); // word 2
    }

    #[test]
    fn fifo_full_write_returns_wait_cycles() {
        // Five rapid data-port writes at the SAME instant on an active-display line (recon R3: 16 slots/line,
        // a VRAM word exits in 2 slots): the first four fill the 4-entry FIFO, the fifth stalls the 68k.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.now_mclk = 100 * crate::vdp::MCLK_PER_LINE + 500; // active line, mid-line
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        vdp_setup_vram_write(&mut bus);
        for _ in 0..4 {
            assert_eq!(
                bus.write16(0xC0_0000, 5, 0xBEEF),
                0,
                "the first four writes fill the FIFO without stalling"
            );
        }
        let wait = bus.write16(0xC0_0000, 5, 0xBEEF);
        assert!(
            wait > 0,
            "the fifth write to a full FIFO stalls the 68k (recon R3)"
        );
    }

    #[test]
    fn fifo_writes_spaced_past_a_slot_do_not_stall() {
        // Writes spaced well past the active VRAM slot cost (~427 mclk) drain between writes — the FIFO never
        // fills, so no write stalls.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.now_mclk = 0;
        {
            let mut sink = Vec::new();
            let mut bus = mem.bus(&mut sink);
            vdp_setup_vram_write(&mut bus);
        }
        for i in 0..8u64 {
            mem.now_mclk = i * 1000; // active lines (0..2), 1000 mclk apart
            let mut sink = Vec::new();
            let mut bus = mem.bus(&mut sink);
            assert_eq!(
                bus.write16(0xC0_0000, 5, 0xBEEF),
                0,
                "a spaced write drains the FIFO first, so it never stalls"
            );
        }
    }

    #[test]
    fn read_waits_for_a_nonempty_write_fifo() {
        // A data-port read while writes are pending stalls the 68k until the write FIFO drains (recon R3:
        // pending writes take priority over reads).
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.now_mclk = 100 * crate::vdp::MCLK_PER_LINE + 500; // active line
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        vdp_setup_vram_write(&mut bus);
        for _ in 0..3 {
            bus.write16(0xC0_0000, 5, 0xBEEF); // load the write FIFO (no stall yet)
        }
        // Switch to a (legal) VRAM read command @ $0000 so the read is not the write-armed lockup.
        bus.write16(0xC0_0004, 5, 0x0000);
        bus.write16(0xC0_0004, 5, 0x0000);
        // A read at the same instant must wait for those 3 pending writes to drain.
        let (_v, wait) = bus.read16(0xC0_0000, 5);
        assert!(
            wait > 0,
            "a read waits for the pending write FIFO to drain (recon R3)"
        );
    }

    /// Program a 68k→VDP (Mem) DMA of `len` words from 68k byte address `src` to VRAM address `dest`, then
    /// fire it by writing the CD5 command. Returns the wait cycles the trigger write reported.
    fn run_mem_dma_to_vram(
        bus: &mut MegaDriveBus<'_, Vec<BusEvent>>,
        src: u32,
        len: u16,
        dest: u16,
    ) -> u32 {
        let sw = src >> 1; // source is programmed as a 68k WORD address (RD3)
        for w in [
            0x8114u16,                           // reg 1: DMA enable (bit4) + mode5, display off
            0x8F02,                              // reg 15: autoinc 2
            0x9300 | (len & 0xFF),               // reg 19: length low
            0x9400 | (len >> 8),                 // reg 20: length high
            0x9500 | (sw as u16 & 0xFF),         // reg 21: source low
            0x9600 | ((sw >> 8) as u16 & 0xFF),  // reg 22: source mid
            0x9700 | ((sw >> 16) as u16 & 0x7F), // reg 23: source high, mode Mem (bit7=0)
        ] {
            bus.write16(0xC0_0004, 5, w);
        }
        bus.write16(0xC0_0004, 5, 0x4000 | (dest & 0x3FFF)); // VRAM-write command word 1 (+CD5 via word 2)
        bus.write16(0xC0_0004, 5, 0x0080 | ((dest >> 14) & 0x3)) // word 2: CD5-2 = 1000 → code $21
    }

    #[test]
    fn mem_dma_copies_source_words_to_vram() {
        let mut rom = vec![0u8; 0x1000];
        rom[0x400..0x408].copy_from_slice(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]);
        let mut mem = MdMem::new(rom);
        mem.now_mclk = 250 * crate::vdp::MCLK_PER_LINE; // vblank line: blanked (fast) transfer
        let mut sink = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);
            run_mem_dma_to_vram(&mut bus, 0x000400, 4, 0x0000);
        }
        assert_eq!(
            &mem.vdp.vram()[0..8],
            &[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88],
            "the 4 source words landed in VRAM big-endian"
        );
    }

    #[test]
    fn mem_dma_updates_the_sat_cache() {
        // R5 pin: "any DMA operation that writes to VRAM also counts" — a Mem DMA into the SAT window updates
        // the cache exactly like a CPU write.
        let mut rom = vec![0u8; 0x1000];
        rom[0x400..0x408].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD, 0x00, 0x00, 0x00, 0x00]);
        let mut mem = MdMem::new(rom);
        mem.now_mclk = 250 * crate::vdp::MCLK_PER_LINE;
        let mut sink = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);
            bus.write16(0xC0_0004, 5, 0x8500); // reg 5 = 0 → SAT base $0000
            run_mem_dma_to_vram(&mut bus, 0x000400, 4, 0x0000);
        }
        assert_eq!(
            &mem.vdp.sat_cache()[0..4],
            &[0xAA, 0xBB, 0xCC, 0xDD],
            "the DMA'd Y + size/link bytes of entry 0 updated the SAT cache"
        );
    }

    #[test]
    fn mem_dma_returns_a_halt_wait_from_the_slot_budget() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.now_mclk = 250 * crate::vdp::MCLK_PER_LINE;
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        let wait = run_mem_dma_to_vram(&mut bus, 0x000400, 16, 0x0000);
        assert!(
            wait > 0,
            "the 68k is halted for the whole transfer (recon R4(a))"
        );
    }

    #[test]
    fn mem_dma_advances_source_and_zeroes_length_registers() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.now_mclk = 250 * crate::vdp::MCLK_PER_LINE;
        let mut sink = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);
            run_mem_dma_to_vram(&mut bus, 0x000400, 4, 0x0000); // source words $0200, +4 → $0204
        }
        let r = mem.vdp.regs();
        assert_eq!((r[0x14], r[0x13]), (0, 0), "length registers zeroed");
        assert_eq!(
            (r[0x17] & 0x7F, r[0x16], r[0x15]),
            (0x00, 0x02, 0x04),
            "source registers advanced to word address $000204"
        );
    }

    #[test]
    fn vdpfifo_t3_dma_payload_walks_the_fifo_ring() {
        // VDPFIFOTesting test 3 "DMA Transfer using FIFO" (`vendor/TestRoms/vdp_port_access.bin`; name
        // string at ROM $5DE8, expected-value table at ROM $5E0C, DMA source payload at ROM $5DDC =
        // `AAAA BBBB CCCC DDDD EEEE FFFF`). This replays the ROM's exact port traffic (disassembled
        // $5E60..$60AC; see docs/2026-08-03-a3-dma-fifo-design.md §1.3) and asserts its exact 16-word
        // hardware-captured answer.
        //
        // The test never reads its DMA destination: all 16 observations are CRAM/VSRAM reads at address 0
        // of a zeroed memory, so every expected bit comes from the undefined-bit FIFO snoop (Nemesis, VDP
        // Internals: undefined bits "are actually initialized to the content on the next available FIFO
        // entry"). It therefore pins exactly one thing — P1: a 68k→VDP DMA's payload words occupy physical
        // FIFO slots, so after the 6-word transfer the ring holds `CCCC DDDD EEEE FFFF` with the write
        // cursor parked on `CCCC`, and each intervening CRAM `$FFFF` write walks the cursor one slot.
        let mut rom = vec![0u8; 0x1000];
        // The DMA source payload, big-endian, at ROM $0400 (word address $000200).
        rom[0x400..0x40C].copy_from_slice(&[
            0xAA, 0xAA, 0xBB, 0xBB, 0xCC, 0xCC, 0xDD, 0xDD, 0xEE, 0xEE, 0xFF, 0xFF,
        ]);
        let mut mem = MdMem::new(rom);
        mem.now_mclk = 250 * crate::vdp::MCLK_PER_LINE; // vblank line: blanked (fast) slot rate
        let mut sink = Vec::new();
        let mut observed = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);
            let ctrl = |bus: &mut MegaDriveBus<'_, Vec<BusEvent>>, w: u16| {
                bus.write16(0xC0_0004, 5, w);
            };
            let data = |bus: &mut MegaDriveBus<'_, Vec<BusEvent>>, w: u16| {
                bus.write16(0xC0_0000, 5, w);
            };

            // ROM $5E60: CRAM write @ $0000, then 64 data writes zero all 128 bytes of CRAM.
            ctrl(&mut bus, 0xC000);
            ctrl(&mut bus, 0x0000);
            for _ in 0..64 {
                data(&mut bus, 0x0000);
            }

            // ROM $5E7A: VRAM write @ $8000, then six marker words into the FIFO.
            ctrl(&mut bus, 0x4000);
            ctrl(&mut bus, 0x0002);
            for w in [0x1000u16, 0x2000, 0x3000, 0x4000, 0x5000, 0x6000] {
                data(&mut bus, w);
            }

            // ROM $5EBE..$5F0C: reg 1 = $54 (display on + M1/DMA enable), reg 15 = 2 (autoinc), regs 19/20
            // = length 6 words, regs 21/22/23 = source word address $000200, mode Mem (reg 23 bit 7 = 0).
            for w in [0x8154u16, 0x8F02, 0x9306, 0x9400, 0x9500, 0x9602, 0x9700] {
                ctrl(&mut bus, w);
            }

            // ROM $5F18: CD = 100001 (VRAM write + CD5) @ $8000 fires the 68k→VDP DMA.
            ctrl(&mut bus, 0x4000);
            ctrl(&mut bus, 0x0082);
            ctrl(&mut bus, 0x8144); // ROM $5F1E: M1 off

            // Four groups of {2 x VSRAM read @0, 2 x CRAM read @0}, each separated by one ring-advancing
            // CRAM data write of $FFFF @ $0020.
            for group in 0..4 {
                ctrl(&mut bus, 0x0000); // CD = 000100 = VSRAM read @ $0000
                ctrl(&mut bus, 0x0010);
                observed.push(bus.read16(0xC0_0000, 5).0);
                observed.push(bus.read16(0xC0_0000, 5).0);
                ctrl(&mut bus, 0x0000); // CD = 001000 = CRAM read @ $0000
                ctrl(&mut bus, 0x0020);
                observed.push(bus.read16(0xC0_0000, 5).0);
                observed.push(bus.read16(0xC0_0000, 5).0);
                if group < 3 {
                    ctrl(&mut bus, 0xC020); // CD = 000011 = CRAM write @ $0020
                    ctrl(&mut bus, 0x0000);
                    data(&mut bus, 0xFFFF);
                }
            }
        }

        assert_eq!(
            observed,
            vec![
                0xc800u16, 0xc800, 0xc000, 0xc000, // snoop = $CCCC
                0xd800, 0xd800, 0xd111, 0xd111, // snoop = $DDDD
                0xe800, 0xe800, 0xe000, 0xe000, // snoop = $EEEE
                0xf800, 0xf800, 0xf111, 0xf111, // snoop = $FFFF
            ],
            "VDPFIFOTesting test 3's expected table (ROM $5E0C)"
        );
    }

    #[test]
    fn vdpfifo_t4_fill_trigger_and_byte_placement() {
        // VDPFIFOTesting test 4 "DMA Fill FIFO Usage" (`vendor/TestRoms/vdp_port_access.bin`; name string
        // at ROM $DC30, expected-value table at ROM $DC54). This replays the ROM's exact port traffic
        // (disassembled $DCA8..$DEAE; see docs/2026-08-03-a3-dma-fifo-design.md §1.6) and asserts its exact
        // 16-word hardware-captured answer.
        //
        // The 16 words split in half. Words 0-7 are VSRAM-read snoop probes: the fill's trigger word takes
        // exactly ONE FIFO slot and the fill's own replicated bytes take none (P4), so the cursor walks
        // `0000 → 0000 → 0000 → $1234 & $F800`. Words 8-15 are the settled VRAM image at $8000, which pins
        // both halves of the fill fix: P2 — the trigger is completed as a NORMAL word write ($12 → $8000,
        // $34 → $8001) and the address then auto-increments — and P3 — the fill engine writes its MSB to
        // `address ^ 1`, so with autoinc 1 the ten steps at $8001..$800A touch
        // {$8000} ∪ {$8002..$8009} ∪ {$800B}, leaving $800A zero. Citations: Nemesis, *VDP Internals*
        // ("that data port write is completed as normal … then pulled out of the FIFO, and processed as a
        // normal FIFO write"); Mask of Destiny, *Is DMA Fill buggy?* ("MSB of the word in the FIFO is
        // written DMA length times to address ^ 1"); Eke, same thread ("VRAM byte writes (used by VRAM fill
        // and copy DMA) actually occur to VRAM address ^ 1").
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.now_mclk = 250 * crate::vdp::MCLK_PER_LINE; // vblank line: blanked (fast) slot rate
        let mut sink = Vec::new();
        let mut observed = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);
            let ctrl = |bus: &mut MegaDriveBus<'_, Vec<BusEvent>>, w: u16| {
                bus.write16(0xC0_0004, 5, w);
            };
            let data = |bus: &mut MegaDriveBus<'_, Vec<BusEvent>>, w: u16| {
                bus.write16(0xC0_0000, 5, w);
            };

            // The ROM reaches test 4 with autoinc 2 and DMA enabled already latched by earlier tests; a
            // fresh VDP needs them set explicitly (reg 1 = $54 is the ROM's own value at $DCFE).
            ctrl(&mut bus, 0x8F02); // reg 15 autoinc 2
            ctrl(&mut bus, 0x8154); // reg 1: display on + M1 (DMA enable) + M5

            // ROM $DCA8: VRAM write @ $8000, then eight data writes zero VRAM $8000..$800F.
            ctrl(&mut bus, 0x4000);
            ctrl(&mut bus, 0x0002);
            for _ in 0..8 {
                data(&mut bus, 0x0000);
            }

            // ROM $DCF6..$DD20: reg 15 = 1 (autoinc 1), regs 19/20 = fill length 10, reg 23 = $80 (fill).
            for w in [0x8F01u16, 0x930A, 0x9400, 0x9780] {
                ctrl(&mut bus, w);
            }

            // ROM $DD56: CD = 100001 (VRAM write + CD5) @ $8000 arms the fill; ROM $DD5C: the data-port
            // write that both triggers it and supplies the fill word.
            ctrl(&mut bus, 0x4000);
            ctrl(&mut bus, 0x0082);
            data(&mut bus, 0x1234);

            ctrl(&mut bus, 0x8F02); // ROM $DD7A: reg 15 back to autoinc 2

            // Four groups of 2 VSRAM reads @0, each separated by one ring-advancing CRAM write of $FFFF.
            for group in 0..4 {
                ctrl(&mut bus, 0x0000); // CD = 000100 = VSRAM read @ $0000
                ctrl(&mut bus, 0x0010);
                observed.push(bus.read16(0xC0_0000, 5).0);
                observed.push(bus.read16(0xC0_0000, 5).0);
                if group < 3 {
                    ctrl(&mut bus, 0xC020); // CD = 000011 = CRAM write @ $0020
                    ctrl(&mut bus, 0x0000);
                    data(&mut bus, 0xFFFF);
                }
            }

            // ROM $DE60: VRAM read @ $8000, eight words (autoinc 2).
            ctrl(&mut bus, 0x0000);
            ctrl(&mut bus, 0x0002);
            for _ in 0..8 {
                observed.push(bus.read16(0xC0_0000, 5).0);
            }
        }

        assert_eq!(
            observed,
            vec![
                0x0000u16, 0x0000, 0x0000, 0x0000, // snoop = a zeroing write
                0x0000, 0x0000, 0x1000, 0x1000, // snoop = the trigger word $1234 & $F800
                0x1234, 0x1212, 0x1212, 0x1212, // VRAM $8000..$8007
                0x1212, 0x0012, 0x0000, 0x0000, // VRAM $8008..$800F
            ],
            "VDPFIFOTesting test 4's expected table (ROM $DC54)"
        );
    }

    #[test]
    fn vdpfifo_t6_eight_bit_vram_read_target() {
        // VDPFIFOTesting test 6 "8-bit VRAM Read target 01100" (`vendor/TestRoms/vdp_port_access.bin`;
        // name string at ROM $DEB0, expected-value table at ROM $DED4 — both loaded by the ROM's own
        // literal `lea`s at $DF04 / $DF18, which is the ground truth for the offsets). This replays the
        // ROM's exact port traffic (disassembled $DF28..$E0F0) and asserts its 16-word hardware answer.
        //
        // The undocumented code CD = %001100 is an 8-bit VRAM read. Every one of the sixteen words pins
        // the same three clauses:
        //   * LOW byte  = `vram[address ^ 1]` — the same byte-lane swap A3b pinned for the fill engine.
        //     Group 5 is the decisive one: autoinc 1 at $8000 reads $22,$11,$44,$33, i.e. bytes
        //     $8001,$8000,$8003,$8002 — a plain `vram[address]` would read $11,$22,$33,$44.
        //   * HIGH byte = the high byte of the next-available FIFO entry (the word written four writes
        //     ago) — the same stale-contents snoop the CRAM/VSRAM undefined bits already read. Groups
        //     1-4 hold the address family fixed while a single ring-advancing CRAM write of $FFFF walks
        //     the snoop $99AA → $BBCC → $DDEE → $1234, and the high byte tracks it exactly.
        //   * the address auto-increments normally, and a read does NOT advance the snoop cursor (both
        //     reads of every 2-read group return the same high byte).
        //
        // Citations: Nemesis, *VDP Internals* (SpritesMind) — undefined result bits "are actually
        // initialized to the content on the next available FIFO entry (the one containing the data
        // written to control port four writes ago)"; Eke, *Is DMA Fill buggy?* (SpritesMind) — "VRAM
        // byte writes … actually occur to VRAM address ^ 1".
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.now_mclk = 250 * crate::vdp::MCLK_PER_LINE; // vblank line: blanked (fast) slot rate
        let mut sink = Vec::new();
        let mut observed = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);
            let ctrl = |bus: &mut MegaDriveBus<'_, Vec<BusEvent>>, w: u16| {
                bus.write16(0xC0_0004, 5, w);
            };
            let data = |bus: &mut MegaDriveBus<'_, Vec<BusEvent>>, w: u16| {
                bus.write16(0xC0_0000, 5, w);
            };

            // The ROM reaches test 6 with autoinc 2 latched by the previous test (every test leaves
            // reg 15 = 2 on exit — test 6's own last act, ROM $E0E8, is `move.w #$8F02`).
            ctrl(&mut bus, 0x8F02);

            // ROM $DF28: VRAM write @ $8000, then eight marker words fill $8000..$800F and leave the
            // last four ($99AA $BBCC $DDEE $1234) in the physical FIFO ring, cursor parked on $99AA.
            ctrl(&mut bus, 0x4000);
            ctrl(&mut bus, 0x0002);
            for w in [
                0x1122u16, 0x3344, 0x5566, 0x7788, 0x99AA, 0xBBCC, 0xDDEE, 0x1234,
            ] {
                data(&mut bus, w);
            }

            // ROM $DF72..$E04E: four groups of two 8-bit VRAM reads (CD = %001100, second control word
            // $0032), at $8000/$8004/$8008/$800C, each group followed by one ring-advancing CRAM write
            // of $FFFF @ $0020 (control $C020 / $0002).
            for lo in [0x0000u16, 0x0004, 0x0008, 0x000C] {
                ctrl(&mut bus, lo);
                ctrl(&mut bus, 0x0032);
                observed.push(bus.read16(0xC0_0000, 5).0);
                observed.push(bus.read16(0xC0_0000, 5).0);
                ctrl(&mut bus, 0xC020);
                ctrl(&mut bus, if lo == 0x000C { 0x0000 } else { 0x0002 });
                data(&mut bus, 0xFFFF);
            }

            // ROM $E062: reg 15 = 1 (autoinc 1). Then four reads @ $8000, a ring-advancing CRAM write,
            // and four more @ $8001 — the odd start address that proves the `^ 1` lane swap.
            ctrl(&mut bus, 0x8F01);
            for lo in [0x0000u16, 0x0001] {
                ctrl(&mut bus, lo);
                ctrl(&mut bus, 0x0032);
                for _ in 0..4 {
                    observed.push(bus.read16(0xC0_0000, 5).0);
                }
                if lo == 0x0000 {
                    ctrl(&mut bus, 0xC020);
                    ctrl(&mut bus, 0x0000);
                    data(&mut bus, 0xFFFF);
                }
            }
        }

        assert_eq!(
            observed,
            vec![
                // snoop $99AA / $BBCC; VRAM bytes $8001 $8003 $8005 $8007
                0x9922u16, 0x9944, 0xBB66, 0xBB88,
                // snoop $DDEE / $1234; VRAM bytes $8009 $800B $800D $800F
                0xDDAA, 0xDDCC, 0x12EE, 0x1234,
                // autoinc 1 @ $8000: VRAM bytes $8001 $8000 $8003 $8002
                0xFF22, 0xFF11, 0xFF44, 0xFF33,
                // autoinc 1 @ $8001: VRAM bytes $8000 $8003 $8002 $8005
                0xFF11, 0xFF44, 0xFF33, 0xFF66,
            ],
            "VDPFIFOTesting test 6's expected table (ROM $DED4)"
        );
    }

    /// Program + trigger a VRAM fill of `len` bytes of `fill`'s top byte at VRAM `dest` (autoinc 1).
    fn run_vram_fill(
        bus: &mut MegaDriveBus<'_, Vec<BusEvent>>,
        dest: u16,
        len: u16,
        fill: u16,
    ) -> u32 {
        for w in [
            0x8114u16,             // reg 1: DMA enable + mode5, display off
            0x8F01,                // reg 15: autoinc 1 (consecutive bytes)
            0x9300 | (len & 0xFF), // reg 19: length low
            0x9400 | (len >> 8),   // reg 20: length high
            0x9780,                // reg 23: fill mode (bits 7-6 = 10)
        ] {
            bus.write16(0xC0_0004, 5, w);
        }
        bus.write16(0xC0_0004, 5, 0x4000 | (dest & 0x3FFF)); // command word 1
        bus.write16(0xC0_0004, 5, 0x0080 | ((dest >> 14) & 0x3)); // word 2 → code $21, CD5
        bus.write16(0xC0_0000, 5, fill) // data-port write triggers the fill; returns its wait
    }

    #[test]
    fn vram_fill_fills_the_target_with_the_top_byte() {
        // A3b rewrite (was: "$0100..$0108 are all $EE"). Two behaviors move the image, both pinned by
        // VDPFIFOTesting test 4 (expected table ROM $DC54): P2 — the trigger word $EEAA is completed as a
        // normal write ($EE → $0100, $AA → $0101) and the address auto-increments — and P3 — each fill byte
        // lands at `address ^ 1`. So the eight fill steps run over addresses $0101..$0108 and write
        // $0100, $0103, $0102, $0105, $0104, $0107, $0106, $0109: $0108 is skipped and $0109 is written.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.now_mclk = 250 * crate::vdp::MCLK_PER_LINE; // vblank
        let skipped_before = mem.vdp.vram()[0x0108];
        let mut sink = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);
            run_vram_fill(&mut bus, 0x0100, 8, 0xEEAA); // fill 8 bytes of $EE
        }
        assert_eq!(
            &mem.vdp.vram()[0x0100..0x0108],
            &[0xEE, 0xAA, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE, 0xEE],
            "the trigger word's LSB survives at $0101; the rest is the fill byte"
        );
        assert_eq!(
            mem.vdp.vram()[0x0108],
            skipped_before,
            "the odd autoincrement skips $0108 entirely"
        );
        assert_eq!(
            mem.vdp.vram()[0x0109],
            0xEE,
            "…and reaches one byte past the naive end instead"
        );
    }

    #[test]
    fn fill_sets_dma_busy_for_the_coarse_window_but_returns_no_wait() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.now_mclk = 250 * crate::vdp::MCLK_PER_LINE;
        let mut sink = Vec::new();
        let wait = {
            let mut bus = mem.bus(&mut sink);
            run_vram_fill(&mut bus, 0x0100, 8, 0xEEAA)
        };
        assert_eq!(wait, 0, "a fill keeps the 68k running (recon R4(b))");
        assert!(
            mem.vdp.dma_busy(mem.now_mclk),
            "DMA-busy set at trigger time"
        );
        assert!(
            !mem.vdp.dma_busy(mem.now_mclk + 1_000_000),
            "DMA-busy clears after the coarse transfer window"
        );
    }

    #[test]
    fn fill_updates_the_sat_cache_on_window_hits() {
        // R5 rider CONFIRMED: fill steps route through the SAT write-through like any VRAM write.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.now_mclk = 250 * crate::vdp::MCLK_PER_LINE;
        let mut sink = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);
            bus.write16(0xC0_0004, 5, 0x8500); // reg 5 = 0 → SAT base $0000
            run_vram_fill(&mut bus, 0x0000, 12, 0x77AA); // fill 12 bytes of $77 from $0000
        }
        // A3b rewrite (was: 4 bytes, asserting `[$77; 4]`). The point of the test is unchanged — every byte
        // the fill path writes still routes through the SAT write-through — but the image moved: the
        // trigger word $77AA is now applied as a normal write ($77 → $0000, $AA → $0001, P2) and the fill
        // steps run over $0001.. writing `address ^ 1` (P3). The length is 12 rather than 4 so that entry
        // 1's cached half (`sat_cache[4..8]`, VRAM $0008..$000B) is covered by **four consecutive fill
        // bytes with no trigger byte among them** — the pure-fill coverage a 4-byte run cannot reach, since
        // `write_vram_byte` only caches `byte_in_entry < 4` and the trigger sits inside entry 0's window.
        assert_eq!(
            &mem.vdp.sat_cache()[0..8],
            &[0x77, 0xAA, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77],
            "fill bytes hit the SAT write-through window compare"
        );
    }

    /// Program + trigger a VRAM copy of `len` bytes from VRAM `source` to VRAM `dest` (autoinc 1). Returns the
    /// trigger control write's wait cycles.
    fn run_vram_copy(
        bus: &mut MegaDriveBus<'_, Vec<BusEvent>>,
        source: u16,
        len: u16,
        dest: u16,
    ) -> u32 {
        for w in [
            0x8114u16,                       // reg 1: DMA enable + mode5, display off
            0x8F01,                          // reg 15: autoinc 1
            0x9300 | (len & 0xFF),           // reg 19: length low
            0x9400 | (len >> 8),             // reg 20: length high
            0x9500 | (source & 0xFF),        // reg 21: VRAM source low
            0x9600 | ((source >> 8) & 0xFF), // reg 22: VRAM source high
            0x97C0,                          // reg 23: copy mode (bits 7-6 = 11)
        ] {
            bus.write16(0xC0_0004, 5, w);
        }
        bus.write16(0xC0_0004, 5, 0x4000 | (dest & 0x3FFF)); // command word 1
        bus.write16(0xC0_0004, 5, 0x0080 | ((dest >> 14) & 0x3)) // word 2 → CD5, triggers the copy
    }

    #[test]
    fn vram_copy_moves_bytes_within_vram() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.now_mclk = 250 * crate::vdp::MCLK_PER_LINE;
        let mut sink = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);
            // Preset VRAM $0200.. = 11 22 33 44 (autoinc 2, word-aligned so no byte-swap surprises).
            bus.write16(0xC0_0004, 5, 0x8F02);
            bus.write16(0xC0_0004, 5, 0x4200);
            bus.write16(0xC0_0004, 5, 0x0000);
            bus.write16(0xC0_0000, 5, 0x1122);
            bus.write16(0xC0_0000, 5, 0x3344);
            run_vram_copy(&mut bus, 0x0200, 4, 0x0100);
        }
        assert_eq!(
            &mem.vdp.vram()[0x0100..0x0104],
            &[0x11, 0x22, 0x33, 0x44],
            "4 bytes copied within VRAM"
        );
    }

    #[test]
    fn copy_keeps_the_68k_running_no_wait() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.now_mclk = 250 * crate::vdp::MCLK_PER_LINE;
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        let wait = run_vram_copy(&mut bus, 0x0200, 4, 0x0100);
        assert_eq!(wait, 0, "a copy keeps the 68k running (recon R4(c))");
    }

    #[test]
    fn copy_updates_the_sat_cache_on_window_hits() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.now_mclk = 250 * crate::vdp::MCLK_PER_LINE;
        let mut sink = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);
            bus.write16(0xC0_0004, 5, 0x8500); // reg 5 = 0 → SAT base $0000
                                               // Preset source VRAM $0200.. = AA BB CC DD.
            bus.write16(0xC0_0004, 5, 0x8F02);
            bus.write16(0xC0_0004, 5, 0x4200);
            bus.write16(0xC0_0004, 5, 0x0000);
            bus.write16(0xC0_0000, 5, 0xAABB);
            bus.write16(0xC0_0000, 5, 0xCCDD);
            run_vram_copy(&mut bus, 0x0200, 4, 0x0000); // copy into SAT entry 0
        }
        assert_eq!(
            &mem.vdp.sat_cache()[0..4],
            &[0xAA, 0xBB, 0xCC, 0xDD],
            "copy writes hit the SAT write-through window compare"
        );
    }

    #[test]
    fn frame_report_lists_the_dma_performed() {
        let mut rom = vec![0u8; 0x1000];
        rom[0x400..0x408].copy_from_slice(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]);
        let mut mem = MdMem::new(rom);
        mem.now_mclk = 250 * crate::vdp::MCLK_PER_LINE;
        assert!(
            mem.vdp.frame_report().dma.is_none(),
            "no DMA before any transfer"
        );
        let mut sink = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);
            run_mem_dma_to_vram(&mut bus, 0x000400, 4, 0xC000);
        }
        let dma = mem
            .vdp
            .frame_report()
            .dma
            .expect("the performed DMA is reported");
        assert_eq!(dma.mode, DmaMode::Mem);
        assert_eq!(dma.dest, 0xC000, "destination address");
        assert_eq!(dma.len, 4, "length");
        assert_eq!(dma.target, Target::Vram);
        assert_eq!(dma.source, 0x000400, "68k source byte address");
    }

    #[test]
    fn flatbus_vdp_address_write_yields_no_wait() {
        // The SST harness bus (FlatBus) has no VDP: a write to the VDP data-port address is plain memory and
        // returns 0 wait forever — the invariant that keeps the SST corpus bit-identical.
        use crate::m68000::bus68k::FlatBus;
        let mut bus = FlatBus::new();
        assert_eq!(
            bus.write16(0xC0_0000, 5, 0xBEEF),
            0,
            "FlatBus word write: no wait"
        );
        assert_eq!(
            bus.write8(0xC0_0000, 5, 0xBE),
            0,
            "FlatBus byte write: no wait"
        );
    }

    #[test]
    fn open_bus_read_returns_the_last_word_driven_by_a_write() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        // The residue source is the last driven word; $400000+ is arbiter-flavored (low byte $00, K4-1).
        bus.write16(0xE0_0010, 5, 0xF00D); // drives 0xF00D onto the bus
        assert_eq!(
            bus.read16(0x40_0000, 6).0,
            0xF000,
            "open bus = last driven word's high byte | $00"
        );
    }

    #[test]
    fn arbiter_open_bus_returns_high_byte_residue_low_byte_driven_00() {
        // K4-1 (docs/2026-08-02-k4-openbus-design.md §3 row 1): a read answered by the arbiter/cart-time
        // side ($400000-$7FFFFF) returns the residue's HIGH byte with the low byte driven to $00 — the
        // memtest hardware column's `4E00` shape — unlike the VDP-side full-word retention (row 13).
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        bus.write16(0xE0_0000, 5, 0xCAFE); // latch := 0xCAFE
        assert_eq!(
            bus.read16(0x40_0000, 6).0,
            0xCA00,
            "word read: high-byte residue, low byte $00"
        );
        assert_eq!(
            bus.read8(0x40_0000, 5).0,
            0xCA,
            "even byte (UDS): the residue half, unchanged"
        );
        assert_eq!(
            bus.read8(0x40_0001, 5).0,
            0x00,
            "odd byte (LDS): driven to $00"
        );
        // The open-bus read itself drives nothing new — the latch is intact for the next read.
        assert_eq!(bus.read16(0x7F_FFFE, 6).0, 0xCA00, "latch unchanged");
    }

    #[test]
    fn a11200_reads_are_undriven_arbiter_open_bus_not_a_latch_readback() {
        // K4-1 (design §3 row 9): `$A11200` reads drive NO lines (the reference arbiter's Z80RESET read
        // handler returns nothing, MDBusArbiter.cpp:448-452) — the memtest hardware column shows `4E00`
        // regardless of the reset toggles. The WRITE latch still works (asserted via `mem.z80_running`,
        // since the readback no longer exists — exactly the point).
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);
            bus.write16(0xA1_1200, 5, 0x0100); // release reset (word idiom)
            bus.write16(0xE0_0000, 5, 0x4E71); // re-drive a known residue word
            assert_eq!(
                bus.read16(0xA1_1200, 5).0,
                0x4E00,
                "read = arbiter open bus, NOT the 0x0100 readback"
            );
            assert_eq!(bus.read8(0xA1_1200, 5).0, 0x4E, "even byte = residue half");
            assert_eq!(bus.read8(0xA1_1201, 5).0, 0x00, "odd byte driven $00");
        }
        assert!(mem.z80_running, "the write latch itself still landed");
    }

    #[test]
    fn mapped_byte_read_merges_only_its_own_lane_into_the_latch() {
        // K4-1 rider (design §4): a byte read drives only its own half of the data bus; the other half
        // keeps floating (Exodus's tri-state merge, M68000.cpp:2138). Replaces the old `b * 0x0101`
        // both-halves smear. Observed through a full-retention open-bus read (ROM past-end).
        let mut rom = vec![0u8; 0x1000];
        rom[0x10] = 0x12;
        let mut mem = MdMem::new(rom);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        bus.write16(0xE0_0000, 5, 0xBEEF); // latch := 0xBEEF
        bus.read8(0x00_0010, 6); // even byte read: drives UDS only → latch = 0x12EF
        assert_eq!(
            bus.read16(0x20_0000, 6).0,
            0x12EF,
            "byte read merged its lane only (was 0x1212 under the smear)"
        );
    }

    #[test]
    fn megadrive_tas_drops_the_write_but_flags_from_the_read() {
        // The Gargoyles/Ex-Mutants quirk: on the Mega Drive the RMW WRITE cycle of TAS is dropped — the read
        // happens, the write does not. So the byte in RAM is UNCHANGED (contrast FlatBus, which stores
        // orig|0x80), while the CPU still gets `orig` back for its flags.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        bus.write8(0xE0_0100, 5, 0x35);
        let (orig, _wait) = bus.tas(0xE0_0100, 5);
        assert_eq!(orig, 0x35, "TAS returns the pre-modify byte for the flags");
        assert_eq!(
            bus.read8(0xE0_0100, 5).0,
            0x35,
            "the Mega Drive drops the TAS write — RAM is UNCHANGED"
        );
        assert!(
            sink.iter().any(|e| e.op == BusOp::Tas),
            "the Tas access is still logged"
        );
    }
}
