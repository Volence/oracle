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
use oracle_core::symbols::{Indeterminate, RomBinding, SymbolTable};
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

    // Park forever.
    //
    // **The socket file outlives this process, and that is a known limitation rather than an
    // oversight.** `ServerHandle::drop` unlinks it, but nothing here ever unwinds: a `SIGINT` or
    // `SIGTERM` kills the process outright, so the path is left behind and the next client to connect
    // gets `ECONNREFUSED` from a dead file rather than `ENOENT` from an absent one. That is confusing —
    // it reads as "the server is broken" rather than "the server is not running" — and it has been
    // reported from a real session.
    //
    // Catching those signals needs either `signal-hook` or `unsafe` `libc`, and this crate's runtime
    // dependency set is deliberately `oracle-core` + `serde_json` and documented as not growing (see
    // `Cargo.toml`), while the library half is `forbid(unsafe_code)`. So the fix is not free and is not
    // taken unilaterally here.
    //
    // What *is* handled, and is what keeps a stale file from being fatal: [`Server::bind`] probes the
    // path before binding — it connects, refuses with `AddrInUse` if a live server answers, and unlinks
    // if nothing does. A stale socket therefore never blocks a restart, which is the half that matters
    // for recovery. `tests/socket_lifecycle.rs` pins both directions.
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
                // The two unverified shapes are different findings and must not share a line: one
                // listing gave us no offset to probe, the other gave us one that says "no appendix
                // here". Same split as `Engine::load_symbols`' caveat.
                match binding {
                    RomBinding::Match { .. } => "bound to this image",
                    RomBinding::Indeterminate(Indeterminate::EndOfRomIsImageEnd { .. }) =>
                        "UNVERIFIED — EndOfRom is the image's end, the no-appendix shape",
                    _ => "UNVERIFIED — no EndOfRom to probe",
                }
            );
            (Some(table), Some(path.display().to_string()))
        }
    }
}
