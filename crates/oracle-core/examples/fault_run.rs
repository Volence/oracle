//! `fault_run` — run a ROM headlessly and **fail if the engine faults**.
//!
//! Built for Aeon's replay net, which is fully implemented and completely dead:
//! `aeon/engine/system/replay.emp` plays an `ARP0` input stream embedded in the ROM, recomputes an
//! address-free hash of the player's gameplay state every 64 ticks, and raises `REPLAY DESYNC` on
//! mismatch — while Aeon's own `DEFERRED_WORK.md` records that *"it cannot detect a desync — that needs
//! the emulator."* This is the emulator half, and it turned out to need **no new emulator capability at
//! all**: the engine plays its own stream, so nothing is injected, and the whole job is noticing that the
//! machine reached its fault handler and reporting the registers the trap carries.
//!
//! Deliberately **not** an Aether client. The recon that ranked this work also recorded that *"a hang in
//! the debug transport destroyed irreplaceable evidence"* — a frozen repro frame lost to a control-socket
//! hang and impossible to re-freeze. A CI gate is exactly where that must not happen, so this drives
//! `oracle-core` directly: no socket, no server, no second process. `motion_run.rs` is the precedent.
//!
//! ```text
//! cargo run --release --example fault_run -- <rom> [--symbols P] [--symbol NAME] [--max-frames N]
//! ```
//!
//! Exit codes are the gate: **0** = ran to the frame bound with no fault; **1** = the fault handler was
//! reached (details on stdout); **2** = usage or setup error. The distinction matters — a runner that
//! exits 0 because it could not resolve its target is a green gate that tests nothing.

use oracle_core::symbols::SymbolTable;
use oracle_core::system::System;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Aeon's `raise_exception` routes to its vendored MD Debugger blob, reached only via
/// `jsr (MDDBG__ErrorHandler).l` — **not** through `ErrorTrap`, which handles the TRAP and reserved
/// vectors and raises `"ERROR TRAP"` of its own. Watching `ErrorTrap` yields a runner that never fires
/// for a desync, which is the failure this default exists to avoid.
const DEFAULT_SYMBOL: &str = "ErrorHandlerBlob";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut rom_path: Option<String> = None;
    let mut symbols: Option<PathBuf> = None;
    let mut symbol = DEFAULT_SYMBOL.to_string();
    let mut addr: Option<u32> = None;
    let mut max_frames: u64 = 3600;

    while let Some(a) = args.next() {
        match a.as_str() {
            "--symbols" => symbols = args.next().map(PathBuf::from),
            // A raw address, for a ROM with no listing — and the only way to aim this at a fixture.
            "--addr" => match args.next().and_then(|v| parse_hex(&v)) {
                Some(a) => addr = Some(a),
                None => return usage("--addr needs a hex address like 0x000280"),
            },
            "--symbol" => match args.next() {
                Some(s) => symbol = s,
                None => return usage("--symbol needs a name"),
            },
            "--max-frames" => match args.next().and_then(|v| v.parse().ok()) {
                Some(n) => max_frames = n,
                None => return usage("--max-frames needs a number"),
            },
            "-h" | "--help" => {
                eprintln!(
                    "usage: fault_run <rom> [--symbols PATH] [--symbol NAME] [--max-frames N]\n\
                     exit 0 = clean, 1 = faulted, 2 = setup error"
                );
                return ExitCode::SUCCESS;
            }
            other if rom_path.is_none() => rom_path = Some(other.to_string()),
            other => return usage(&format!("unexpected argument: {other}")),
        }
    }
    let Some(rom_path) = rom_path else {
        return usage("a ROM path is required");
    };
    let rom = match std::fs::read(&rom_path) {
        Ok(b) => b,
        Err(e) => return usage(&format!("cannot read ROM {rom_path}: {e}")),
    };

    // Resolve the fault handler BEFORE running. A target that cannot be resolved is a setup error, never
    // a clean run: the alternative is a gate that passes because it was watching nothing.
    let (target, what) = match addr {
        Some(a) => (a, "--addr".to_string()),
        None => {
            let lst = symbols.unwrap_or_else(|| Path::new(&rom_path).with_extension("lst"));
            match resolve(&lst, &rom, &symbol) {
                Ok(a) => (a, symbol.clone()),
                Err(e) => return usage(&e),
            }
        }
    };
    println!("fault_run: watching {what} at 0x{target:06X}, up to {max_frames} frames");

    let mut sys = System::new(0x5EED);
    sys.load_rom(rom);
    sys.reset();

    // The predicate sees the PC of each instruction *before* it commits, so the stop lands on the handler's
    // first instruction with the fault frame still intact.
    let stop = sys.run_until_stop(max_frames, |pc, _frame| pc == target);

    if !stop.fired() {
        println!("fault_run: CLEAN — {max_frames} frames, {what} never reached");
        return ExitCode::SUCCESS;
    }

    let r = sys.cpu_regs();
    println!(
        "fault_run: FAULT at frame {} (mclk {})",
        stop.frame, stop.mclk
    );
    println!("  pc  0x{:06X}  (the handler, not yet executed)", stop.pc);
    // Aeon's desync path carries actual/tick/expected in d0/d1/d2 — printed for every fault because a
    // fault handler reached by any other route still has registers worth seeing, and guessing which
    // handler this is would be the runner inventing knowledge it does not have.
    println!(
        "  d0  0x{:08X}   d1  0x{:08X}   d2  0x{:08X}",
        r.d[0], r.d[1], r.d[2]
    );
    println!("  (Aeon REPLAY DESYNC convention: d0 = actual hash, d1 = Logic_Tick, d2 = expected)");
    ExitCode::from(1)
}

fn parse_hex(v: &str) -> Option<u32> {
    let t = v
        .trim_start_matches("0x")
        .trim_start_matches("0X")
        .trim_start_matches('$');
    u32::from_str_radix(t, 16).ok()
}

fn usage(msg: &str) -> ExitCode {
    eprintln!("fault_run: {msg}");
    ExitCode::from(2)
}

/// Resolve `symbol` in the listing beside the ROM, refusing a listing that does not describe this image —
/// the same policy the server applies, for the same reason: a listing from another build is not degraded
/// information, it is confidently wrong information, and here it would aim the watch at an address that
/// means nothing in this ROM.
fn resolve(lst: &Path, rom: &[u8], symbol: &str) -> Result<u32, String> {
    let text = std::fs::read_to_string(lst).map_err(|e| {
        format!(
            "cannot read symbols {} ({e}) — needed to resolve {symbol}",
            lst.display()
        )
    })?;
    let table = SymbolTable::parse(&text)
        .map_err(|e| format!("{} is not a usable listing: {e}", lst.display()))?;
    if matches!(
        table.validate_against_rom(rom),
        oracle_core::symbols::RomBinding::Mismatch(_)
    ) {
        return Err(format!(
            "REFUSED — {} does not describe this ROM image",
            lst.display()
        ));
    }
    table
        .address_of(symbol)
        .ok_or_else(|| format!("no symbol named {symbol} in {}", lst.display()))
}
