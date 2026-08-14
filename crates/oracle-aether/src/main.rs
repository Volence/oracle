//! `oracle-aether` — serve one ROM on the Aether bus.
//!
//! ```text
//! oracle-aether <rom.bin> [--socket PATH] [--symbols PATH] [--no-pace]
//! ```
//!
//! The socket path follows `protocol.md` §7.1 when `--socket` is omitted: `$ORACLE_SOCKET` →
//! `$EXODUS_SOCKET` (transitional) → `$XDG_RUNTIME_DIR/oracle.sock` → `/tmp/oracle.sock`.
//!
//! Symbols are opt-in by presence: without `--symbols`, the `<rom>.lst` beside the ROM is loaded if it
//! exists and **refused** if it does not bind to the image (recon §9b/§9g) — a listing from a different
//! build shape is not degraded information, it is confidently wrong information.

use oracle_aether::server::{default_socket_path, Machine, Server, ServerConfig};
use oracle_core::symbols::{RomBinding, SymbolTable};
use oracle_core::system::System;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut rom_path: Option<String> = None;
    let mut socket: Option<PathBuf> = None;
    let mut symbols: Option<PathBuf> = None;
    let mut pace = true;

    while let Some(a) = args.next() {
        match a.as_str() {
            "--socket" => socket = args.next().map(PathBuf::from),
            "--symbols" => symbols = args.next().map(PathBuf::from),
            "--no-pace" => pace = false,
            "-h" | "--help" => {
                eprintln!(
                    "usage: oracle-aether <rom.bin> [--socket PATH] [--symbols PATH] [--no-pace]"
                );
                return ExitCode::SUCCESS;
            }
            other if rom_path.is_none() => rom_path = Some(other.to_string()),
            other => {
                eprintln!("unexpected argument: {other}");
                return ExitCode::from(2);
            }
        }
    }

    let Some(rom_path) = rom_path else {
        eprintln!("usage: oracle-aether <rom.bin> [--socket PATH] [--symbols PATH] [--no-pace]");
        return ExitCode::from(2);
    };
    let rom = match std::fs::read(&rom_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot read ROM {rom_path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!("rom: {rom_path} ({} bytes)", rom.len());

    let mut sys = System::new(0x5EED);
    sys.load_rom(rom.clone());
    sys.reset();

    let lst = symbols.unwrap_or_else(|| Path::new(&rom_path).with_extension("lst"));
    let (table, table_path) = load_symbols(&lst, &rom);

    let mut config = ServerConfig {
        socket_path: socket.unwrap_or_else(default_socket_path),
        ..ServerConfig::default()
    };
    if !pace {
        config.engine.free_run_pace = None;
    }

    let server = match Server::bind(config) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot bind the Aether socket: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "aether: listening on {} (mode 0600, protocol version {})",
        server.socket_path().display(),
        oracle_aether::rpc::PROTOCOL_VERSION
    );
    println!(
        "aether: {} methods advertised",
        oracle_aether::engine::METHODS.len()
    );

    let mut machine = Machine::new(sys);
    machine.rom_path = Some(rom_path);
    machine.symbols = table;
    machine.symbols_path = table_path;
    let handle = server.spawn(machine);

    // Park forever; the handle's Drop unlinks the socket if this ever unwinds.
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
        std::hint::black_box(&handle);
    }
}

/// The startup half of D7. Same policy as the frontend's `symbol_file.rs`: absent is fine, unparseable
/// is a warning, and a listing that does not bind to the loaded image is **refused** rather than used.
fn load_symbols(path: &Path, rom: &[u8]) -> (Option<SymbolTable>, Option<String>) {
    let Ok(text) = std::fs::read_to_string(path) else {
        println!(
            "symbols: none at {} (running with raw addresses)",
            path.display()
        );
        return (None, None);
    };
    let table = match SymbolTable::parse(&text) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("symbols: {} is not a usable listing ({e})", path.display());
            return (None, None);
        }
    };
    match table.validate_against_rom(rom) {
        RomBinding::Mismatch(fault) => {
            eprintln!(
                "symbols: REFUSED — {} does not describe this ROM image ({fault:?})",
                path.display()
            );
            (None, None)
        }
        RomBinding::Indeterminate(_) if !table.is_intact() => {
            eprintln!(
                "symbols: REFUSED — {} cannot be bound to the ROM and is not internally intact",
                path.display()
            );
            (None, None)
        }
        binding => {
            println!(
                "symbols: {} symbols from {} ({})",
                table.len(),
                path.display(),
                match binding {
                    RomBinding::Match { .. } => "bound to this image",
                    _ => "UNVERIFIED — no EndOfRom to probe",
                }
            );
            (Some(table), Some(path.display().to_string()))
        }
    }
}
