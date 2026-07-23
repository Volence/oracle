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

/// Split-borrow adapter implementing the CPU-facing [`Bus68k`] over the `System`'s memory fields laid out per
/// the real Mega Drive map, emitting a [`BusEvent`] (with the real function code) per access. The CPU core
/// cannot tell it apart from the SST harness's `FlatBus` — the point of the unification. Every 24-bit address
/// has a deterministic answer; open bus returns the last word driven on the bus (`last_bus_word`). Writes
/// apply immediately in this pivot (no other master is live); the deferred-write seam plugs in with the VDP.
///
/// | Range | Behavior |
/// |---|---|
/// | `$000000–$3FFFFF` | ROM (read-only; past a short ROM's end → open bus) |
/// | `$400000–$7FFFFF` | open bus |
/// | `$A00000–$A0FFFF` | 8 KiB Z80 RAM (mirrored), except… |
/// | `$A04000–$A04003` | YM2612 FM: read = status (bit7 BUSY clear = not busy); writes drop (no FM core yet) |
/// | `$A10000–$A1001F` | I/O: `$A10001` = [`MD_VERSION`]; the 15 data/control/serial registers via [`Io`] |
/// | `$A11100` | Z80 BUSREQ: bit0 read = 0 when 68000 is granted the bus (asserted), 1 when the Z80 owns it |
/// | `$A11200` | Z80 RESET: bit0 = the reset-release latch (`z80_running`) — write 1 = release (Z80 runs), 0 = assert (held); read reports it (power-on 0 = held) |
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
            fm,
            sink,
        }
    }

    /// The byte backing a mapped address, or `None` for open bus (unmapped ranges, past a short ROM's end,
    /// the VDP data port). Real memory (ROM / work RAM / Z80 RAM) and the fixed-constant registers return
    /// `Some`; the caller substitutes the open-bus latch for `None`.
    fn mapped_byte(&self, a: u32) -> Option<u8> {
        match a {
            // ROM: bytes present up to the ROM's length; past the end is open bus (no mirroring assumed).
            0x00_0000..=0x3F_FFFF => {
                let i = a as usize;
                (i < self.rom.len()).then(|| self.rom[i])
            }
            // YM2612 FM ($A04000-$A04003): a READ returns the live status byte — Timer-A overflow (bit0),
            // Timer-B overflow (bit1), and bit7 (BUSY) always clear = not busy, so the `btst #7,(a0)/bne`
            // register-write busy-poll still exits (recon F2/F4, DR-1b Gunstar). The overflow flags are a pure
            // function of the timers + the current mclk (docs/2026-07-22-fm-timer-design.md). This arm precedes
            // the Z80-RAM mirror so the FM ports are not aliased to `z80_ram[0..3]`.
            0xA0_4000..=0xA0_4003 => Some(self.fm.read_status(self.now_mclk)),
            // Z80 RAM: 8 KiB mirrored across the 64 KiB window.
            0xA0_0000..=0xA0_FFFF => Some(self.z80_ram[(a as usize) & (Z80_RAM_SIZE - 1)]),
            // I/O ($A10000–$A1001F): the fixed version byte at $A10001, the 15 data/control/serial registers
            // via the `Io` model (recon IO1–IO4), or 0 for the even (unmapped high-half) bytes.
            0xA1_0001 => Some(MD_VERSION),
            0xA1_0000..=0xA1_001F => Some(match io_reg(a) {
                Some((port, IoReg::Data)) => self.io.read_data(port),
                Some((port, IoReg::Ctrl)) => self.io.read_ctrl(port),
                Some((port, IoReg::TxData)) => self.io.read_txdata(port),
                Some((port, IoReg::SCtrl)) => self.io.read_sctrl(port),
                // RxData: no serial device drives the receive line (decision 2). Even bytes: unmapped → 0.
                Some((_, IoReg::RxData)) | None => 0x00,
            }),
            // Z80 BUSREQ ($A11100): bit0 reports bus ownership — 0 = 68000 granted (BUSREQ asserted), 1 = Z80
            // owns the bus (released). Drives the take-bus (`bne`, wait for 0) and release (`beq`, wait for 1)
            // spins real games use (recon Z2/Z5). $A11101 (odd half) = 0.
            0xA1_1100 => Some(if *self.z80_busreq { 0x00 } else { 0x01 }),
            0xA1_1101 => Some(0x00),
            // Z80 RESET ($A11200): bit0 reports the reset-release latch (`z80_running`) — 1 = released (Z80
            // runs), 0 = asserted (held). Power-on = 0 (held), byte-identical to the old constant-0 stub, so
            // this is currency-neutral until a game writes bit0 = 1 (recon Z1, design ZC6). $A11201 half = 0.
            0xA1_1200 => Some(if *self.z80_running { 0x01 } else { 0x00 }),
            0xA1_1201 => Some(0x00),
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
            // YM2612 FM ($A04000-$A04003): writes drive the FM chip's address-latch/data protocol (timer regs
            // $24/$25/$26/$27 update the timer model; everything else is ignored — no synthesis this slice).
            // They must NOT fall through to the Z80-RAM store, which would corrupt `z80_ram[0..3]` (the FM
            // ports alias it under the mirror). This arm precedes the Z80-RAM arm (recon F1/DR-1b, Option B).
            0xA0_4000..=0xA0_4003 => self.fm.write_port(a as u16, byte, self.now_mclk),
            0xA0_0000..=0xA0_FFFF => self.z80_ram[(a as usize) & (Z80_RAM_SIZE - 1)] = byte,
            // Z80 BUSREQ ($A11100): latch bit0 from the EVEN byte only — a word write (`move.w #$100/#$0`)
            // puts the meaningful byte at $A11100 and 0 at $A11101, which must not clobber the latch. $A11101
            // and $A11200 (RESET, deferred to Z7) fall through and drop (recon Z1/Z5).
            0xA1_1100 => *self.z80_busreq = (byte & 1) != 0,
            // Z80 RESET ($A11200): latch bit0 from the EVEN byte only (a word write `move.w #$100,$A11200`
            // puts the meaningful byte at $A11200, 0 at $A11201). 1 = release reset (Z80 runs), 0 = assert
            // (held). $A11201 falls through and drops. No committed fixture writes bit0 = 1 (design ZC13).
            0xA1_1200 => *self.z80_running = (byte & 1) != 0,
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
            0xC0_0004 | 0xC0_0006 => (self.vdp.control_read_status(self.now_mclk), 0),
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
        let value = if let Some(hi) = self.mapped_byte(a) {
            let lo = self
                .mapped_byte((a.wrapping_add(1)) & ADDR_MASK)
                .unwrap_or(0);
            let v = ((hi as u16) << 8) | lo as u16;
            *self.last_bus_word = v; // a real word crossed the bus
            v
        } else {
            *self.last_bus_word // open bus: the last word driven, unchanged
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
        self.store_byte(a, (value >> 8) as u8);
        self.store_byte((a.wrapping_add(1)) & ADDR_MASK, (value & 0xFF) as u8);
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
            *self.last_bus_word = (b as u16) * 0x0101; // byte driven on both halves (placeholder open-bus rule)
            b
        } else if a & 1 == 0 {
            (*self.last_bus_word >> 8) as u8 // even address → UDS half
        } else {
            (*self.last_bus_word & 0xFF) as u8 // odd address → LDS half
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
        bus.write16(0xE0_0000, 5, 0xCAFE); // latch := 0xCAFE
        assert_eq!(
            bus.read16(0x50_0000, 6).0,
            0xCAFE,
            "unmapped read = last word"
        );
        // A second open-bus read still sees the same latch (an open-bus read does not drive a new word).
        assert_eq!(bus.read16(0x60_0000, 6).0, 0xCAFE);
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

    #[test]
    fn z80_ram_reads_writes_and_mirrors_in_its_window() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        bus.write8(0xA0_0000, 5, 0x9A);
        assert_eq!(bus.read8(0xA0_0000, 5).0, 0x9A, "Z80 RAM byte round-trips");
        // 8 KiB Z80 RAM mirrored across the 64 KiB window.
        assert_eq!(bus.read8(0xA0_2000, 5).0, 0x9A, "mirror at +0x2000");
    }

    #[test]
    fn version_register_returns_the_fixed_constant() {
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        assert_eq!(bus.read8(0xA1_0001, 5).0, MD_VERSION, "version register");
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
    fn z80_reset_latch_powers_on_asserted_and_toggles() {
        // `$A11200` bit0 is the Z80 reset-release latch (`z80_running`): power-on = reset ASSERTED (Z80
        // held), a write of bit0 = 1 releases it (Z80 runs), a write of bit0 = 0 re-asserts it. The read
        // reports the latch. Promotes the old constant-0/drop stub to a real latch (design ZC6/ZC13). The
        // power-on read value (0) is byte-identical to the old stub, so the change is currency-neutral.
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);

        // Power-on: reset asserted, Z80 held → read bit0 = 0 (matches the old stub exactly).
        assert_eq!(
            bus.read8(0xA1_1200, 5).0 & 1,
            0,
            "power-on -> reset asserted (bit0 = 0)"
        );

        // Release reset via the word idiom `move.w #$100,$A11200`: $01 lands at the even address, $00 at the
        // odd one; the odd byte must NOT clobber the latch.
        bus.write16(0xA1_1200, 5, 0x0100);
        assert_eq!(
            bus.read8(0xA1_1200, 5).0 & 1,
            1,
            "reset released -> Z80 runs (bit0 = 1)"
        );

        // Re-assert via `move.w #$0,$A11200`: bit0 returns to 0 (held again).
        bus.write16(0xA1_1200, 5, 0x0000);
        assert_eq!(
            bus.read8(0xA1_1200, 5).0 & 1,
            0,
            "reset re-asserted -> Z80 held (bit0 = 0)"
        );

        // The byte-write idiom releases too.
        bus.write8(0xA1_1200, 5, 0x01);
        assert_eq!(
            bus.read8(0xA1_1200, 5).0 & 1,
            1,
            "byte release -> Z80 runs (bit0 = 1)"
        );
        // The odd half always reads 0.
        assert_eq!(bus.read8(0xA1_1201, 5).0, 0, "$A11201 odd half = 0");
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

        // Real Z80 RAM at $A00000 still round-trips — the carve-out is $A04000-3 only.
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
        // Line 100 (active), H=0x40 (dot 1280) — not v/h blank, so only the FIFO-empty placeholder is set.
        mem.now_mclk = 100 * crate::vdp::MCLK_PER_LINE + 1280;
        let expected = mem.vdp.status_word(mem.now_mclk);
        let mut sink = Vec::new();
        let mut bus = mem.bus(&mut sink);
        assert_eq!(bus.read16(0xC0_0004, 5).0, expected, "live VDP status word");
        assert_eq!(expected, 0x0200, "FIFO-empty only during active display");
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
        let mut mem = MdMem::new(vec![0u8; 0x1000]);
        mem.now_mclk = 250 * crate::vdp::MCLK_PER_LINE; // vblank
        let mut sink = Vec::new();
        {
            let mut bus = mem.bus(&mut sink);
            run_vram_fill(&mut bus, 0x0100, 8, 0xEEAA); // fill 8 bytes of $EE
        }
        assert!(
            mem.vdp.vram()[0x0100..0x0108].iter().all(|&b| b == 0xEE),
            "8 consecutive VRAM bytes filled with the top byte $EE"
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
            run_vram_fill(&mut bus, 0x0000, 4, 0x77AA); // fill 4 bytes of $77 into entry 0
        }
        assert_eq!(
            &mem.vdp.sat_cache()[0..4],
            &[0x77, 0x77, 0x77, 0x77],
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
        bus.write16(0xE0_0010, 5, 0xF00D); // drives 0xF00D onto the bus
        assert_eq!(
            bus.read16(0x40_0000, 6).0,
            0xF00D,
            "open bus = last driven word"
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
