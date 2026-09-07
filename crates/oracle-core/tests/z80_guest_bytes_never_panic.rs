//! **No guest byte may panic the emulator.** The containment gate for the Z80 core.
//!
//! Until the `Z80Fault` refusal landed, three `unimplemented!()` sites in `z80/mod.rs` covered **560 real
//! Z80 encodings** — the undocumented `ED` mirrors (20), the `IXH`/`IXL` half-register forms (46 under each
//! of `DD` and `FD`), and the `DDCB`/`FDCB` register-copy variants (224 under each prefix). None of them was
//! contained: `catch_unwind` appears nowhere in the shipping crates, so one byte in a guest's Z80 program
//! killed the thread. In the player the window died mid-session; in the server the engine thread died while
//! the socket stayed bound — an outage indistinguishable from a hang. The in-tree precedent is Vectorman's
//! `FD FF`, which pinned the emulator at 26 frames until the prefix rule landed.
//!
//! This test is that measurement, run as a gate. It executes **every** encoding in the base table and in all
//! five prefixed tables under `catch_unwind` and requires:
//!
//! 1. **Zero panics.** Any panic is a byte a ROM could feed us that kills the process.
//! 2. **A refusal is never a degradation.** An encoding the core does not serve must latch a `Z80Fault`
//!    naming its exact bytes — never quietly execute as something else. All 560 are now *implemented* and
//!    corpus-graded, so this branch currently has nothing to catch; it stays because it is what makes a
//!    future deferral visible here instead of silent.
//! 3. **A refusal does not corrupt the machine.** `PC` is rewound to the instruction's first byte, every
//!    other architectural register is byte-identical to before the step (except `R`, whose M1 refresh bumps
//!    really happened), and not one byte of RAM moved.
//!
//! The latch/diagnostic behaviour of `Z80Fault` itself lives in `z80/mod.rs`'s own test module, where a
//! refusal can be raised directly — no guest byte can produce one any more.
//!
//! `catch_unwind` here is the **detector**, not the containment. The containment is structural: the decode
//! arms refuse instead of panicking. That distinction is the whole design — a `catch_unwind` wrapper in the
//! shipping path would catch the symptom while leaving the machine half-stepped, and would evaporate
//! silently under `panic = "abort"`.

use oracle_core::z80::{Z80Io, Z80Regs, Z80};
use std::panic::{self, AssertUnwindSafe};

/// Where each encoding under test is planted, and where a refusal must leave `PC`.
const ENTRY: u16 = 0x0100;

/// The signed displacement used for the `DDCB`/`FDCB` encodings. Non-zero on purpose, so `(IX+d)` is a
/// different address from `IX` and a stray write would be visible.
const DISPLACEMENT: u8 = 0x05;

/// **How many encodings this core does not serve.** It started at 560 — the three classes three
/// `unimplemented!()` sites covered — and each landing class dropped it, the drop being that class's proof
/// of arrival:
///
/// - ~~**20** undocumented `ED` mirrors~~ — **LANDED**, graded 20/20 against the corpus.
/// - ~~**46 × 2** `IXH`/`IXL` half-register forms~~ — **LANDED**, graded 92/92 against the corpus.
/// - ~~**224 × 2** `DDCB`/`FDCB` register-copy variants~~ — **LANDED**, graded 448/448 against the corpus.
///
/// **It is now zero**, which is a strictly stronger statement than "nothing panics": every encoding in the
/// base table and all five prefixed tables *executes*. Should a future slice defer an encoding again, it
/// raises this number and lands in `Z80::refuse` — never in a panic.
const EXPECTED_UNSERVED: usize = 0;

/// A flat 64 KiB Z80 address space plus an open-bus port model — the same isolation the SST-z80 runner
/// uses (a bare `Z80` over a flat bus, never `System`), so this cannot touch any frozen currency.
struct FlatBus {
    ram: Vec<u8>,
    ports_written: Vec<(u16, u8)>,
}

impl FlatBus {
    fn new() -> Self {
        Self {
            ram: vec![0u8; 0x1_0000],
            ports_written: Vec::new(),
        }
    }
}

impl Z80Io for FlatBus {
    fn read(&mut self, addr: u16) -> u8 {
        self.ram[addr as usize]
    }
    fn write(&mut self, addr: u16, value: u8) {
        self.ram[addr as usize] = value;
    }
    fn input(&mut self, _port: u16) -> u8 {
        0xFF // the Genesis leaves the Z80 I/O space unused; open bus reads $FF
    }
    fn output(&mut self, port: u16, value: u8) {
        self.ports_written.push((port, value));
    }
}

/// A deliberately non-trivial starting register state, so an encoding that *does* execute actually moves
/// something and an encoding that refuses has plenty of state to be caught corrupting. Every pair differs,
/// so a wrong-register substitution (`LD B,IXH` executed as `LD B,H`) cannot hide behind equal values.
fn seed_regs() -> Z80Regs {
    Z80Regs {
        a: 0x5A,
        f: 0x00,
        b: 0x11,
        c: 0x22,
        d: 0x33,
        e: 0x44,
        h: 0x55,
        l: 0x66,
        af_: 0x1234,
        bc_: 0x2345,
        de_: 0x3456,
        hl_: 0x4567,
        ix: 0x2000,
        iy: 0x3000,
        sp: 0x8000,
        pc: ENTRY,
        i: 0x77,
        r: 0x10,
        iff1: true,
        iff2: true,
        im: 1,
        halted: false,
        wz: 0,
        q: 0,
    }
}

/// The four prefix bytes. Excluded from the base table and from the `DD`/`FD` tables for the same reason
/// the corpus has no `dd cb.json`: they are not opcodes, they are an instruction that has not finished. A
/// chain like `DD CB` continues into whatever follows in RAM, so its *label* would not name the encoding
/// actually executed. Chains get their own no-panic test below.
const PREFIXES: [u8; 4] = [0xCB, 0xDD, 0xED, 0xFD];

/// Every **complete** encoding this gate steps, as `(label, bytes)`: the base table and all five prefixed
/// tables, 1524 encodings. Not just the ones expected to refuse — a gate that only visits the known-bad set
/// cannot notice a new panic appearing in the served ones.
///
/// `ED xx` is complete for every `xx` (no `ED` opcode is itself a prefix), and so are the four-byte
/// `DDCB`/`FDCB` forms; the base and `DD`/`FD` tables drop the four [`PREFIXES`].
fn all_encodings() -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for op in 0x00u16..=0xFF {
        let op = op as u8;
        if PREFIXES.contains(&op) {
            continue;
        }
        out.push((format!("{op:02x}"), vec![op]));
    }
    for op in 0x00u16..=0xFF {
        let op = op as u8;
        out.push((format!("ed {op:02x}"), vec![0xED, op]));
    }
    for (prefix, tag) in [(0xDDu8, "dd"), (0xFD, "fd")] {
        for op in 0x00u16..=0xFF {
            let op = op as u8;
            if PREFIXES.contains(&op) {
                continue;
            }
            out.push((format!("{tag} {op:02x}"), vec![prefix, op]));
        }
    }
    for (prefix, tag) in [(0xDDu8, "dd cb"), (0xFD, "fd cb")] {
        for op in 0x00u16..=0xFF {
            let op = op as u8;
            out.push((
                format!("{tag} __ {op:02x}"),
                vec![prefix, 0xCB, DISPLACEMENT, op],
            ));
        }
    }
    out
}

/// Which prefixed table an encoding belongs to — for the per-class breakdown the log prints. Classified
/// from the **bytes under test**, never from the core's decode, so the breakdown is an independent reading.
fn class_of(bytes: &[u8]) -> &'static str {
    match bytes {
        [0xED, ..] => "ED",
        [0xDD, 0xCB, ..] => "DDCB",
        [0xFD, 0xCB, ..] => "FDCB",
        [0xDD, ..] => "DD",
        [0xFD, ..] => "FD",
        _ => "base",
    }
}

#[test]
fn no_guest_byte_panics_the_z80_core() {
    let encodings = all_encodings();
    assert_eq!(
        encodings.len(),
        (256 - PREFIXES.len()) + 256 + 2 * (256 - PREFIXES.len()) + 2 * 256,
        "leg count: base + ED + DD + FD + DDCB + FDCB, complete encodings only"
    );

    // Silence the default hook: a passing run of this gate would otherwise print nothing, but a *failing*
    // one would bury its own report under hundreds of backtraces. The payloads are recorded below.
    //
    // ⚑ Nothing inside this window may `assert!`: the hook is global, so an assertion here would be
    // swallowed and the failure would print as a bare `FAILED` with no reason. Findings are collected into
    // `problems` and asserted after the hook is restored.
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));

    let mut panicked: Vec<String> = Vec::new();
    let mut problems: Vec<String> = Vec::new();
    let mut refused: Vec<(String, oracle_core::z80::Z80Fault)> = Vec::new();
    let mut executed = 0usize;

    for (label, bytes) in &encodings {
        let before = seed_regs();
        let mut z80 = Z80::from_regs(&before);
        let mut bus = FlatBus::new();
        for (i, b) in bytes.iter().enumerate() {
            bus.ram[ENTRY as usize + i] = *b;
        }
        let pristine_ram = bus.ram.clone();

        let stepped = panic::catch_unwind(AssertUnwindSafe(|| {
            z80.step(&mut bus);
        }));

        if stepped.is_err() {
            panicked.push(label.clone());
            continue;
        }

        match z80.fault() {
            None => executed += 1,
            Some(fault) => {
                let after = z80.regs();
                let mut expected = before;
                // M1 refresh bumps really happened; everything else must not move.
                expected.r = after.r;
                // (2) The refusal names the exact bytes it refused.
                if fault.bytes[..fault.len as usize] != bytes[..] {
                    problems.push(format!(
                        "[{label}] the latched fault names {:02X?}, not the encoding it refused",
                        &fault.bytes[..fault.len as usize]
                    ));
                }
                // (3) The refusal did not corrupt the machine.
                if fault.pc != ENTRY {
                    problems.push(format!(
                        "[{label}] the fault names PC {:#06X}, not the instruction's FIRST byte {ENTRY:#06X}",
                        fault.pc
                    ));
                }
                if after.pc != ENTRY {
                    problems.push(format!(
                        "[{label}] PC left at {:#06X}, not rewound to the refused byte {ENTRY:#06X}",
                        after.pc
                    ));
                }
                if after != expected {
                    problems.push(format!(
                        "[{label}] a refusal moved architectural state: {after:?} vs {expected:?}"
                    ));
                }
                if bus.ram != pristine_ram {
                    problems.push(format!("[{label}] a refusal wrote to RAM"));
                }
                if !bus.ports_written.is_empty() {
                    problems.push(format!(
                        "[{label}] a refusal drove the I/O ports: {:?}",
                        bus.ports_written
                    ));
                }
                refused.push((label.clone(), fault));
            }
        }
    }

    panic::set_hook(previous_hook);

    // (1) The headline. Reported as a full list, never a tail excerpt.
    assert!(
        panicked.is_empty(),
        "{} guest encoding(s) PANICKED the Z80 core — a ROM containing any of these bytes kills the \
         process: {panicked:?}",
        panicked.len()
    );
    assert!(
        problems.is_empty(),
        "{} refusal(s) violated the no-corruption contract:\n  {}",
        problems.len(),
        problems.join("\n  ")
    );

    let mut by_class: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for (_, fault) in &refused {
        *by_class
            .entry(class_of(&fault.bytes[..fault.len as usize]))
            .or_default() += 1;
    }
    eprintln!(
        "Z80 CONTAINMENT: {} encodings stepped, 0 panicked, {} executed, {} refused by name {by_class:?}",
        encodings.len(),
        executed,
        refused.len()
    );

    assert_eq!(
        refused.len(),
        EXPECTED_UNSERVED,
        "the unserved-encoding count moved; see EXPECTED_UNSERVED for how it is derived. Refused: \
         {by_class:?}"
    );
    assert_eq!(
        executed,
        encodings.len(),
        "with EXPECTED_UNSERVED at zero, every encoding must EXECUTE — not merely fail to panic"
    );
    assert_eq!(
        executed + refused.len(),
        encodings.len(),
        "every encoding must either execute or refuse — there is no third outcome"
    );
}

/// **Prefix chains cannot panic either.** The sweep above steps complete encodings; a guest is free to
/// write `DD DD FD ED 4C` and the decoder's recursion must survive it. Every two-prefix combination is
/// stepped here, followed by every base opcode — the shape that produced the in-tree precedent (Vectorman's
/// `FD FF`) — with only the no-panic claim asserted, since a chain's *label* does not name the encoding
/// that finally executes.
#[test]
fn prefix_chains_never_panic() {
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));

    let mut panicked: Vec<String> = Vec::new();
    let mut legs = 0usize;
    for a in PREFIXES {
        for b in PREFIXES {
            for op in 0x00u16..=0xFF {
                let op = op as u8;
                let mut z80 = Z80::from_regs(&seed_regs());
                let mut bus = FlatBus::new();
                // The chain, then a displacement/op tail so a DDCB continuation has bytes to read.
                for (i, byte) in [a, b, op, DISPLACEMENT, op].iter().enumerate() {
                    bus.ram[ENTRY as usize + i] = *byte;
                }
                legs += 1;
                if panic::catch_unwind(AssertUnwindSafe(|| {
                    z80.step(&mut bus);
                }))
                .is_err()
                {
                    panicked.push(format!("{a:02x} {b:02x} {op:02x}"));
                }
            }
        }
    }

    panic::set_hook(previous_hook);
    assert_eq!(legs, PREFIXES.len() * PREFIXES.len() * 256, "leg count");
    assert!(
        panicked.is_empty(),
        "{} prefix chain(s) PANICKED the Z80 core: {panicked:?}",
        panicked.len()
    );
    eprintln!("Z80 CONTAINMENT: {legs} prefix chains stepped, 0 panicked");
}
