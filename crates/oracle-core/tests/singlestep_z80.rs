//! SingleStepTests runner for the Z80 core (the sound-driver host).
//!
//! Drives the pinned, vendored SingleStepTests/z80 corpus (`tools/fetch-z80-tests.sh`; data gitignored under
//! `vendor/ProcessorTests/z80/v1`) — the Oracle-independent gate for the Z80 (the analog of
//! `singlestep_m68000.rs`). Each `.json` is 1000 `{initial, final, cycles}` cases for one opcode. This slice
//! covers the opcodes the fetch script pulls: the **entire un-prefixed base table** (every opcode `00`-`ff`
//! except the four prefix bytes `cb`/`dd`/`ed`/`fd`), the **full CB-prefixed group** (`"cb 00"`-`"cb ff"`:
//! the rotates/shifts, `BIT`/`RES`/`SET`), PLUS the **documented ED-prefixed subset** (`"ed 40"` … : the
//! 16-bit arithmetic/loads, `NEG`, `RETN`/`RETI`, `IM`, the `I`/`R` loads, `RRD`/`RLD`, `IN r,(C)`/
//! `OUT (C),r`, and the block transfer/search/I/O groups — see `ED_OPCODES`).
//!
//! **Structurally isolated** (ZC10): it instantiates a bare [`Z80`] + a flat 64 KiB [`Z80TestBus`] and **never**
//! [`System`](oracle_core::system::System), so it cannot touch any frozen currency — identically to how the
//! 68000 SST runner drives `Cpu68000` + `FlatBus`. It skips cleanly if the vendor dir is absent.
//!
//! **What is gated (ZC1 instruction-atomic model + the design's UPDATE-2026-07-22 note):**
//! - The per-case comparison gates on the `final` **register + touched-RAM** state and **ignores** the
//!   per-cycle `cycles` bus trace — our instruction-atomic core does not reproduce sub-instruction bus cycles.
//! - **Documented-flag mode** (the default, this slice's gate, ZC11): everything is asserted exactly — all
//!   main + shadow registers, `pc`, `sp`, `i`, `r`, `iff1`/`iff2`, `im`, the documented flag bits
//!   `S Z H P/V N C`, and RAM — **except** the undocumented flag bits 5/3 (`YF`/`XF`) and the `wz`/`q`
//!   registers, which stay inert until the ZEXALL follow-up. Flipping [`STRICT_FLAGS`] on turns the excluded
//!   state (all flag bits + `wz` + `q`) into hard assertions — the named, defaulted-off path a future
//!   (undocumented-accuracy) slice enables.

use oracle_core::z80::{Z80Io, Z80Regs, Z80};
use serde_json::Value;
use std::path::Path;

const VENDOR_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../vendor/ProcessorTests/z80/v1"
);

/// Documented-flag mode is this slice's gate: `YF`/`XF` (flag bits 5/3) and `wz`/`q` are excluded from the
/// comparison (they stay inert in the first executing version, ZC8/ZC11). Setting this to `true` turns them
/// into hard assertions — the strict, all-flags + `wz`/`q` path a future undocumented-accuracy slice enables.
const STRICT_FLAGS: bool = false;

/// The undocumented flag bits (`YF` = bit 5, `XF` = bit 3), excluded from the documented-flag comparison.
const UNDOC_FLAGS: u8 = 0b0010_1000;

/// Opcode files driven this slice (keep in sync with `tools/fetch-z80-tests.sh`'s `FILES`): the **entire
/// un-prefixed base table** — every opcode `0x00`-`0xFF` except the four prefix bytes `0xCB`/`0xDD`/`0xED`/
/// `0xFD` — PLUS the **full CB-prefixed group** (`"cb 00"`-`"cb ff"`: the rotates/shifts
/// `RLC`/`RRC`/`RL`/`RR`/`SLA`/`SRA`/`SLL`/`SRL`, `BIT b`, `RES b`, `SET b`, across all eight targets). The
/// `DD`/`ED`/`FD`/`DDCB`/`FDCB` tables are the later prefix-group slices.
fn opcode_files() -> Vec<String> {
    let base = (0x00u16..=0xFF)
        .filter(|op| !matches!(op, 0xCB | 0xDD | 0xED | 0xFD))
        .map(|op| format!("{op:02x}"));
    let cb = (0x00u16..=0xFF).map(|op| format!("cb {op:02x}"));
    let ed = ED_OPCODES.iter().map(|op| format!("ed {op:02x}"));
    base.chain(cb).chain(ed).collect()
}

/// The documented ED-prefixed opcodes covered by sub-slice 5 (keep in sync with `tools/fetch-z80-tests.sh`'s
/// `ED_OPS`): `IN r,(C)`/`OUT (C),r`, `SBC`/`ADC HL,rr`, `LD (nn),rr`/`LD rr,(nn)`, `NEG`, `RETN`/`RETI`,
/// `IM 0/1/2`, `LD I,A`/`R,A`/`A,I`/`A,R`, `RRD`/`RLD`, and the block transfer/search/I/O groups. The
/// undocumented ED holes/mirrors (`0x70`/`0x71` etc.) and the `DD`/`FD` prefixes are later slices.
const ED_OPCODES: [u8; 58] = [
    0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4d, 0x4f, //
    0x50, 0x51, 0x52, 0x53, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5b, 0x5e, 0x5f, //
    0x60, 0x61, 0x62, 0x63, 0x67, 0x68, 0x69, 0x6a, 0x6b, 0x6f, //
    0x72, 0x73, 0x78, 0x79, 0x7a, 0x7b, //
    0xa0, 0xa1, 0xa2, 0xa3, 0xa8, 0xa9, 0xaa, 0xab, //
    0xb0, 0xb1, 0xb2, 0xb3, 0xb8, 0xb9, 0xba, 0xbb, //
];

/// A flat 64 KiB Z80 address space (ZC10) — plain array (the SST corpus is pure memory) plus a port model
/// for `IN`/`OUT`. Seeded from each case's `initial.ram`; touched bytes are read back for the `final.ram`
/// check. Port I/O is serviced from the case's `ports` list: the ordered `"r"` (read) values feed each
/// `IN`, and every `OUT` is captured and compared against the `"w"` (write) entries.
struct Z80TestBus {
    ram: Vec<u8>,
    /// Expected `IN` responses (the case's `ports` `"r"` values, in order), popped front-to-back.
    port_reads: std::collections::VecDeque<u8>,
    /// Captured `OUT` writes `(port, value)`, compared against the case's `ports` `"w"` entries.
    port_writes: Vec<(u16, u8)>,
}

impl Z80TestBus {
    fn new() -> Self {
        Self {
            ram: vec![0u8; 0x1_0000],
            port_reads: std::collections::VecDeque::new(),
            port_writes: Vec::new(),
        }
    }
}

impl Z80Io for Z80TestBus {
    fn read(&mut self, addr: u16) -> u8 {
        self.ram[addr as usize]
    }
    fn write(&mut self, addr: u16, value: u8) {
        self.ram[addr as usize] = value;
    }
    fn input(&mut self, _port: u16) -> u8 {
        self.port_reads
            .pop_front()
            .expect("case supplies an IN port-read value for every IN executed")
    }
    fn output(&mut self, port: u16, value: u8) {
        self.port_writes.push((port, value));
    }
}

fn u16f(v: &Value, key: &str) -> u16 {
    v.get(key)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("missing field {key}")) as u16
}

fn u8f(v: &Value, key: &str) -> u8 {
    v.get(key)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("missing field {key}")) as u8
}

fn boolf(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_u64).unwrap_or(0) != 0
}

/// Build a [`Z80Regs`] from an SST `initial`/`final` state object (main regs individual, shadow as pairs).
fn build_regs(s: &Value) -> Z80Regs {
    Z80Regs {
        a: u8f(s, "a"),
        f: u8f(s, "f"),
        b: u8f(s, "b"),
        c: u8f(s, "c"),
        d: u8f(s, "d"),
        e: u8f(s, "e"),
        h: u8f(s, "h"),
        l: u8f(s, "l"),
        af_: u16f(s, "af_"),
        bc_: u16f(s, "bc_"),
        de_: u16f(s, "de_"),
        hl_: u16f(s, "hl_"),
        ix: u16f(s, "ix"),
        iy: u16f(s, "iy"),
        sp: u16f(s, "sp"),
        pc: u16f(s, "pc"),
        i: u8f(s, "i"),
        r: u8f(s, "r"),
        iff1: boolf(s, "iff1"),
        iff2: boolf(s, "iff2"),
        im: u8f(s, "im"),
        halted: false, // SST has no halted-input field; every case starts non-halted.
        wz: u16f(s, "wz"),
        q: u8f(s, "q"),
    }
}

fn seed_ram(bus: &mut Z80TestBus, s: &Value) {
    for pair in s["ram"].as_array().unwrap() {
        let p = pair.as_array().unwrap();
        let addr = p[0].as_u64().unwrap() as usize;
        bus.ram[addr] = p[1].as_u64().unwrap() as u8;
    }
}

fn assert_final(name: &str, got: &Z80Regs, bus: &Z80TestBus, fin: &Value) {
    let want = build_regs(fin);
    // Registers asserted in every mode.
    assert_eq!(got.a, want.a, "A [{name}]");
    assert_eq!(got.b, want.b, "B [{name}]");
    assert_eq!(got.c, want.c, "C [{name}]");
    assert_eq!(got.d, want.d, "D [{name}]");
    assert_eq!(got.e, want.e, "E [{name}]");
    assert_eq!(got.h, want.h, "H [{name}]");
    assert_eq!(got.l, want.l, "L [{name}]");
    assert_eq!(got.af_, want.af_, "AF' [{name}]");
    assert_eq!(got.bc_, want.bc_, "BC' [{name}]");
    assert_eq!(got.de_, want.de_, "DE' [{name}]");
    assert_eq!(got.hl_, want.hl_, "HL' [{name}]");
    assert_eq!(got.ix, want.ix, "IX [{name}]");
    assert_eq!(got.iy, want.iy, "IY [{name}]");
    assert_eq!(got.sp, want.sp, "SP [{name}]");
    assert_eq!(got.pc, want.pc, "PC [{name}]");
    assert_eq!(got.i, want.i, "I [{name}]");
    assert_eq!(got.r, want.r, "R [{name}]");
    assert_eq!(got.iff1, want.iff1, "IFF1 [{name}]");
    assert_eq!(got.iff2, want.iff2, "IFF2 [{name}]");
    assert_eq!(got.im, want.im, "IM [{name}]");

    // Flags: documented mode masks off the undocumented YF/XF (bits 5/3); strict mode asserts all 8 bits.
    if STRICT_FLAGS {
        assert_eq!(got.f, want.f, "F (all bits) [{name}]");
        assert_eq!(got.wz, want.wz, "WZ [{name}]");
        assert_eq!(got.q, want.q, "Q [{name}]");
    } else {
        assert_eq!(
            got.f & !UNDOC_FLAGS,
            want.f & !UNDOC_FLAGS,
            "F (documented S Z H P/V N C) [{name}]: got {:#010b} want {:#010b}",
            got.f,
            want.f
        );
    }

    // Touched RAM.
    for pair in fin["ram"].as_array().unwrap() {
        let p = pair.as_array().unwrap();
        let addr = p[0].as_u64().unwrap() as usize;
        let val = p[1].as_u64().unwrap() as u8;
        assert_eq!(bus.ram[addr], val, "ram[{addr:#06x}] [{name}]");
    }
}

/// Parse a case's top-level `ports` list into (ordered `IN` read values, expected `OUT` writes). Each entry
/// is `[port, value, "r"|"w"]`: `"r"` feeds an `IN`, `"w"` is an `OUT` the core must reproduce.
fn split_ports(t: &Value) -> (Vec<u8>, Vec<(u16, u8)>) {
    let mut reads = Vec::new();
    let mut writes = Vec::new();
    if let Some(list) = t.get("ports").and_then(Value::as_array) {
        for p in list {
            let e = p.as_array().unwrap();
            let port = e[0].as_u64().unwrap() as u16;
            let val = e[1].as_u64().unwrap() as u8;
            match e[2].as_str().unwrap() {
                "r" => reads.push(val),
                "w" => writes.push((port, val)),
                other => panic!("unknown port direction {other:?}"),
            }
        }
    }
    (reads, writes)
}

fn run_case(t: &Value) {
    let name = t["name"].as_str().unwrap_or("?");
    let ini = &t["initial"];
    let mut z80 = Z80::from_regs(&build_regs(ini));
    let mut bus = Z80TestBus::new();
    seed_ram(&mut bus, ini);
    let (reads, expected_writes) = split_ports(t);
    bus.port_reads = reads.into();

    let _t_states = z80.step(&mut bus); // `cycles` trace intentionally ignored (ZC1 instruction-atomic).

    assert_final(name, &z80.regs(), &bus, &t["final"]);
    assert_eq!(bus.port_writes, expected_writes, "port OUT writes [{name}]");
}

/// CI guard: the vendored corpus MUST be present under CI so a fetch regression fails loudly instead of the
/// whole Z80 SST suite skipping and passing vacuously (mirrors the 68000 harness's guard).
#[test]
fn vendor_data_present_when_running_in_ci() {
    if std::env::var_os("CI").is_none() {
        return;
    }
    assert!(
        Path::new(VENDOR_DIR).exists(),
        "CI: vendored SingleStepTests/z80 dir {VENDOR_DIR} is missing — tools/fetch-z80-tests.sh must run \
         before the test job (a missing dir makes the whole Z80 SST suite skip and pass vacuously)"
    );
    for f in opcode_files() {
        let p = format!("{VENDOR_DIR}/{f}.json");
        assert!(
            Path::new(&p).exists(),
            "CI: vendored Z80 SST file {p} is missing — tools/fetch-z80-tests.sh did not fetch the full set"
        );
    }
}

#[test]
fn z80_matches_singlesteptests() {
    if !Path::new(VENDOR_DIR).exists() {
        eprintln!("SKIP: {VENDOR_DIR} missing — run tools/fetch-z80-tests.sh");
        return;
    }
    let mut total = 0usize;
    for fname in opcode_files() {
        let path = format!("{VENDOR_DIR}/{fname}.json");
        if !Path::new(&path).exists() {
            eprintln!("SKIP: {path} missing — run tools/fetch-z80-tests.sh");
            continue;
        }
        let file = std::fs::File::open(&path).unwrap();
        let data: Vec<Value> = serde_json::from_reader(std::io::BufReader::new(file)).unwrap();
        for t in &data {
            run_case(t);
        }
        eprintln!("  {fname}.json: {} cases passed", data.len());
        total += data.len();
    }
    // 252 base-table files + 256 CB-prefixed files + 58 documented ED-prefixed files = 566 opcode files ×
    // 1000 cases. The base table is the 256 opcodes minus the four prefix bytes 0xCB/0xDD/0xED/0xFD; the CB
    // group is "cb 00".."cb ff"; the ED subset is the 58 documented ED opcodes (see `ED_OPCODES`).
    assert_eq!(
        total, 566_000,
        "expected 566000 Z80 SST cases (base table 252k + CB group 256k + documented ED subset 58k)"
    );
}
