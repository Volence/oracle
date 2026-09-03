//! **`emulator/object_spawn` / `_move` / `_delete`** — `protocol.md` §6's three object MUTATION rows,
//! adopted 2026-09-03 as §11.32 (CR-J), served here.
//!
//! # The consumer is a test double, and that is what makes §8 item 25 provable at all
//!
//! Item 25 requires this suite to show **a refusal by name against a build lacking a mailbox symbol,
//! with no write**, and **a non-zero engine status reaching the client as a typed error, proven with at
//! least the pool-full and stale-handle cases**. The second half cannot be shown against a game ROM
//! that happens to be lying around: it needs the engine to *refuse*, on demand, one refusal at a time.
//!
//! So the fixture below is a **68000 program that implements aeon's side of the mailbox protocol** —
//! hand-assembled here, no toolchain, in `oracle_core::testrom`'s own idiom. It polls the flag, copies
//! the payload it saw at the moment the flag went non-zero into a witness area, writes whatever status
//! the test scripted, publishes a handle, and clears the flag **last**. That buys three things a real
//! ROM cannot:
//!
//! 1. **Every engine status on demand**, including the ones a real game reaches once in a thousand
//!    frames.
//! 2. **A witness for flag-last.** The double copies the payload when it first sees the flag set, so a
//!    server that wrote the flag before the payload would be caught by the *engine* rather than by a
//!    test reading the server's own source.
//! 3. **A record the double never moves**, which is the only way to tell the reply's `x`/`y`
//!    RE-READ from an echo of the request. Against a real ROM the two agree on a stationary object and
//!    the test proves nothing; here the record says `(7, 9)` while the request said `(1111, 2222)`, and
//!    only one of those two readings can produce the right answer.
//!
//! **The addresses below are the fixture's own, deliberately NOT aeon's.** The server resolves every
//! cell by name on every call, so a test that reused the real build's numbers could pass against a
//! server that had them baked in — which is the property under test.
//!
//! What this file cannot show, and what did show it: the rows working against the **real** mailbox in a
//! real game state. That was measured firsthand and is banked in `docs/2026-09-02-cr-spawn-mode.md`
//! §17.2, against an aeon build that is not a repository fixture here.

mod common;

use common::{spawn_with, Client};
use serde_json::{json, Value};

// ---------------------------------------------------------------------------------------------------
// The fixture's memory map — its own numbers, not the game's
// ---------------------------------------------------------------------------------------------------

const POOL_BASE: u32 = 0x00FF_8000;
const SST: u32 = 0x50;
const NUM_PLAYERS: u32 = 2;
const NUM_DYNAMIC: u32 = 40;
const NUM_SYSTEM: u32 = 8;
const NUM_EFFECTS: u32 = 16;
const NUM_TOTAL: u32 = NUM_PLAYERS + NUM_DYNAMIC + NUM_SYSTEM + NUM_EFFECTS;
const OBJ_CODE_BASE: u32 = 0x0001_0000;

/// The mailbox, in the engine's declaration order and at its declared widths.
const MB: u32 = 0x00FF_9600;
const MB_DEF: u32 = MB;
const MB_X: u32 = MB + 4;
const MB_Y: u32 = MB + 6;
const MB_SLOT: u32 = MB + 8;
const MB_PLACE: u32 = MB + 10;
const MB_OP: u32 = MB + 12;
const MB_STATUS: u32 = MB + 13;
const MB_FLAG: u32 = MB + 14;

/// Cells the **test** drives and the double reads. Not part of the mailbox and never named to the
/// server.
const SCRIPT_STATUS: u32 = 0x00FF_9700;
const SCRIPT_HANDLE: u32 = 0x00FF_9702;
const DEAF: u32 = 0x00FF_9704;

/// What the double saw at the moment it observed the flag set.
const W_DEF: u32 = 0x00FF_9710;
const W_X: u32 = 0x00FF_9714;
const W_Y: u32 = 0x00FF_9716;
const W_SLOT: u32 = 0x00FF_9718;
const W_PLACE: u32 = 0x00FF_971A;
const W_OP: u32 = 0x00FF_971C;

/// The dynamic pool's first slot, and the handle that names it.
fn dynamic_slot(n: u32) -> u32 {
    NUM_PLAYERS + n
}
fn slot_addr(slot: u32) -> u32 {
    POOL_BASE + slot * SST
}
fn handle_of(slot: u32) -> String {
    format!("0x{:04X}", slot_addr(slot) & 0xFFFF)
}

// ---------------------------------------------------------------------------------------------------
// The 68000 test double
// ---------------------------------------------------------------------------------------------------

const ROM_LEN: usize = 0x300;
const CODE: u32 = 0x0000_0200;

fn put_word(rom: &mut [u8], at: u32, w: u16) {
    rom[at as usize] = (w >> 8) as u8;
    rom[at as usize + 1] = (w & 0xFF) as u8;
}
fn put_long(rom: &mut [u8], at: u32, l: u32) {
    put_word(rom, at, (l >> 16) as u16);
    put_word(rom, at + 2, (l & 0xFFFF) as u16);
}

/// A one-instruction emitter that keeps the program counter and the byte stream in one place, so the
/// short branches below are computed rather than counted by hand.
struct Asm {
    rom: Vec<u8>,
    pc: u32,
}

impl Asm {
    fn op(&mut self, w: u16) -> &mut Self {
        put_word(&mut self.rom, self.pc, w);
        self.pc += 2;
        self
    }
    fn long(&mut self, l: u32) -> &mut Self {
        put_long(&mut self.rom, self.pc, l);
        self.pc += 4;
        self
    }
    /// `MOVE.<sz> (src).l, (dst).l` — absolute long to absolute long.
    fn mov(&mut self, size_op: u16, src: u32, dst: u32) -> &mut Self {
        self.op(size_op).long(src).long(dst)
    }
    /// `TST.B (addr).l`
    fn tst_b(&mut self, addr: u32) -> &mut Self {
        self.op(0x4A39).long(addr)
    }
    /// `CLR.B (addr).l`
    fn clr_b(&mut self, addr: u32) -> &mut Self {
        self.op(0x4239).long(addr)
    }
    /// A short conditional/unconditional branch to an already-emitted label.
    fn br(&mut self, opcode_hi: u16, target: u32) -> &mut Self {
        let disp = target as i64 - (self.pc as i64 + 2);
        assert!(
            (-128..=127).contains(&disp),
            "short branch out of range: {disp}"
        );
        self.op(opcode_hi | ((disp as i8) as u8 as u16))
    }
}

const MOVE_B: u16 = 0x13F9;
const MOVE_W: u16 = 0x33F9;
const MOVE_L: u16 = 0x23F9;
const BRA_S: u16 = 0x6000;
const BEQ_S: u16 = 0x6700;
const BNE_S: u16 = 0x6600;

/// The ROM: aeon's consumer, reduced to what the protocol says and nothing else.
///
/// ```text
/// loop:  tst.b  DEAF          ; scripted deafness — the mailboxNotConsumed case
///        bne.s  loop
///        tst.b  Obj_Req_Flag  ; the only thing that starts a consumption
///        beq.s  loop
///        <copy Def/X/Y/Slot/Place/Op into the witness area>
///        move.w SCRIPT_HANDLE, Obj_Req_Slot     ; the engine publishes the handle …
///        move.b SCRIPT_STATUS, Obj_Req_Status   ; … then the status …
///        clr.b  Obj_Req_Flag                    ; … and the flag LAST. The ack.
///        bra.s  loop
/// ```
fn consumer_rom() -> Vec<u8> {
    let mut a = Asm {
        rom: vec![0u8; ROM_LEN],
        pc: CODE,
    };
    put_long(&mut a.rom, 0x0000, 0x00FF_FFFE); // initial SSP
    put_long(&mut a.rom, 0x0004, CODE); // initial PC
    a.op(0x46FC).op(0x2700); // move.w #$2700, SR — supervisor, interrupts masked
    let loop_top = a.pc;
    a.tst_b(DEAF).br(BNE_S, loop_top);
    a.tst_b(MB_FLAG).br(BEQ_S, loop_top);
    a.mov(MOVE_L, MB_DEF, W_DEF);
    a.mov(MOVE_W, MB_X, W_X);
    a.mov(MOVE_W, MB_Y, W_Y);
    a.mov(MOVE_W, MB_SLOT, W_SLOT);
    a.mov(MOVE_W, MB_PLACE, W_PLACE);
    a.mov(MOVE_B, MB_OP, W_OP);
    a.mov(MOVE_W, SCRIPT_HANDLE, MB_SLOT);
    a.mov(MOVE_B, SCRIPT_STATUS, MB_STATUS);
    a.clr_b(MB_FLAG);
    a.br(BRA_S, loop_top);
    assert!(a.pc < ROM_LEN as u32, "the double outgrew its ROM");
    a.rom
}

// ---------------------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------------------

/// The object-pool rows every layout needs, computed rather than listed.
fn pool_rows() -> Vec<(String, u32)> {
    let player = POOL_BASE;
    let dynamic = player + NUM_PLAYERS * SST;
    let system = dynamic + NUM_DYNAMIC * SST;
    let effect = system + NUM_SYSTEM * SST;
    vec![
        ("Object_RAM".into(), player),
        ("Player_1".into(), player),
        ("Player_2".into(), player + SST),
        ("Dynamic_Slots".into(), dynamic),
        ("System_Slots".into(), system),
        ("Effect_Slots".into(), effect),
        ("Object_RAM_End".into(), player + NUM_TOTAL * SST),
        ("ObjCodeBase".into(), OBJ_CODE_BASE),
    ]
}

/// The eight mailbox cells, in the engine's declaration order.
fn mailbox_rows() -> Vec<(String, u32)> {
    vec![
        ("Obj_Req_Def".into(), MB_DEF),
        ("Obj_Req_X".into(), MB_X),
        ("Obj_Req_Y".into(), MB_Y),
        ("Obj_Req_Slot".into(), MB_SLOT),
        ("Obj_Req_Place".into(), MB_PLACE),
        ("Obj_Req_Op".into(), MB_OP),
        ("Obj_Req_Status".into(), MB_STATUS),
        ("Obj_Req_Flag".into(), MB_FLAG),
    ]
}

fn listing(rows: &[(String, u32)]) -> String {
    let mut s = String::from("  Symbol Table (* = unused):\n\n");
    for (name, addr) in rows {
        s.push_str(&format!(" {name} : {addr:X} C |\n"));
    }
    s.push_str(&format!("\n{:>4} symbols\n", rows.len()));
    s
}

fn load_listing(c: &mut Client, tag: &str, rows: &[(String, u32)]) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("oracle-objmut-{}-{tag}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{tag}.lst"));
    std::fs::write(&path, listing(rows)).unwrap();
    c.ok(
        "emulator/load_symbols",
        json!({"path": path.to_str().unwrap()}),
    );
}

struct Fixture {
    _h: oracle_aether::server::ServerHandle,
    c: Client,
}

/// A server on the test double, paused, with the pool **and** the mailbox in its symbol table.
fn fixture(tag: &str) -> Fixture {
    let h = spawn_with(tag, consumer_rom(), 64);
    let mut c = Client::connect(&h);
    c.handshake(false);
    c.ok("emulator/pause", json!({}));
    let mut rows = pool_rows();
    rows.extend(mailbox_rows());
    load_listing(&mut c, tag, &rows);
    script(&mut c, 0, 0);
    Fixture { _h: h, c }
}

/// A server on the test double whose symbol table names the pool but **not** the mailbox — a release
/// ROM, as far as these rows can tell.
fn fixture_without_mailbox(tag: &str) -> Fixture {
    let h = spawn_with(tag, consumer_rom(), 64);
    let mut c = Client::connect(&h);
    c.handshake(false);
    c.ok("emulator/pause", json!({}));
    load_listing(&mut c, tag, &pool_rows());
    poke(&mut c, DEAF, 0, 1);
    Fixture { _h: h, c }
}

fn poke(c: &mut Client, addr: u32, value: u64, width: u32) {
    c.ok(
        "emulator/write_memory",
        json!({"addr": format!("0x{addr:08X}"), "value": value, "width": width}),
    );
}

fn peek(c: &mut Client, addr: u32, len: u32) -> Vec<u8> {
    let r = c.ok(
        "emulator/read_memory",
        json!({"addr": format!("0x{addr:08X}"), "len": len}),
    );
    let s = r["bytes"].as_str().expect("bytes");
    (0..(s.len() - 2) / 2)
        .map(|i| u8::from_str_radix(&s[2 + i * 2..4 + i * 2], 16).unwrap())
        .collect()
}

/// What the double will answer with, and the handle it will publish.
///
/// **`DEAF` is written every time, and that is not belt-and-braces.** `System::new` seeds work RAM with
/// a pattern rather than zeroes, so an unwritten script cell is whatever the seed put there — and a
/// non-zero `DEAF` parks the double in its own spin loop, which reaches every test in this file as
/// `mailboxNotConsumed`. Measured the hard way: fourteen of twenty-one rows red on one uninitialised
/// byte.
fn script(c: &mut Client, status: u8, handle: u32) {
    poke(c, DEAF, 0, 1);
    poke(c, SCRIPT_STATUS, u64::from(status), 1);
    poke(c, SCRIPT_HANDLE, u64::from(handle & 0xFFFF), 2);
}

/// Make a slot hold a live record at a known position, so a re-read has something to find that the
/// request did not put there.
fn seat_record(c: &mut Client, slot: u32, x: u16, y: u16) {
    let a = slot_addr(slot);
    poke(c, a, 0x27DE, 2); // code_addr — non-zero IS the activity test
    poke(c, a + 2, u64::from(x), 2); // x, integer half of the 16.16
    poke(c, a + 6, u64::from(y), 2); // y
}

/// The work-RAM range every "it wrote nothing" control fingerprints: the object pool, the mailbox
/// and the witness area in one span.
///
/// A **range**, not the cells the refusal is about. Reading back `Obj_Req_Flag` to prove a refused
/// row did not write it would only prove the refusal knew where the flag was — and the fault this
/// guards against is a server that computed an address from an offset and wrote fifteen bytes
/// somewhere else entirely.
fn ram_window() -> Value {
    json!({"addr": format!("0x{POOL_BASE:08X}"), "len": 0x2000})
}

fn err_of(v: &Value) -> (i64, String, Value) {
    (
        v["code"].as_i64().expect("code"),
        v["message"].as_str().unwrap_or_default().to_string(),
        v["data"].clone(),
    )
}

// ---------------------------------------------------------------------------------------------------
// 1. §8 item 25, first half — a refusal BY NAME, and no write
// ---------------------------------------------------------------------------------------------------

/// **A build with no mailbox is refused by name, and every absent name is listed.**
///
/// `-32013` and not `-32012`: a table *is* loaded (the pool decodes fine), it simply has no
/// `Obj_Req_*` in it — which is what a release ROM looks like from here, and the message has to say so
/// because the client's next question is always *why not*.
#[test]
fn a_build_without_the_mailbox_is_refused_by_name_on_all_three_rows() {
    let mut f = fixture_without_mailbox("nomb");
    for (method, params) in [
        (
            "emulator/object_spawn",
            json!({"def": "0x00000200", "x": 1, "y": 1}),
        ),
        (
            "emulator/object_move",
            json!({"handle": handle_of(dynamic_slot(0)), "x": 1, "y": 1}),
        ),
        (
            "emulator/object_delete",
            json!({"handle": handle_of(dynamic_slot(0))}),
        ),
    ] {
        let (code, message, data) = err_of(&f.c.err(method, params));
        assert_eq!(code, -32013, "{method}");
        let missing: Vec<String> = data["missing"]
            .as_array()
            .unwrap_or_else(|| panic!("{method}: data.missing"))
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            missing.len(),
            8,
            "{method}: EVERY absent name, not the first — a partial answer to \"what is missing\" \
             invites a fix-and-retry loop"
        );
        assert!(
            missing.contains(&"Obj_Req_Flag".to_string()),
            "{method}: {missing:?}"
        );
        assert!(
            message.contains("DEBUG"),
            "{method}: the message must say a release ROM resolves none of it: {message}"
        );
    }
}

/// **…and it wrote nothing.** The safety property is not that the call fails; it is that fifteen bytes
/// did not land at a computed address in a build whose RAM means something else there. Proven by the
/// machine's own fingerprint over the whole of work RAM, not by reading the cells the refusal is about.
#[test]
fn a_refused_row_does_not_touch_the_machine() {
    let mut f = fixture_without_mailbox("nowrite");
    let before = f.c.ok("emulator/memory_hash", ram_window());
    f.c.err(
        "emulator/object_spawn",
        json!({"def": "0x00000200", "x": 1234, "y": 5678, "subtype": 9}),
    );
    let after = f.c.ok("emulator/memory_hash", ram_window());
    assert_eq!(
        before["hash"], after["hash"],
        "a symbol-refused mutation row moved work RAM; the refusal is the safety property and it leaked"
    );
    assert_eq!(before["frame"], after["frame"], "and it advanced no frames");
}

// ---------------------------------------------------------------------------------------------------
// 2. §8 item 25, second half — every non-zero engine status as a typed error
// ---------------------------------------------------------------------------------------------------

/// **The whole point of the rows.** The engine's refusals are silent by construction — it writes a
/// status byte and clears the flag exactly as a success does — so a server that reported one as
/// `result` would look correct on every success path. Each is scripted on the double and each must
/// arrive as an error, with the code and the discriminant §11.32 names.
#[test]
fn every_non_zero_engine_status_reaches_the_client_as_a_typed_error() {
    let mut f = fixture("status");
    seat_record(&mut f.c, dynamic_slot(0), 7, 9);
    let h = handle_of(dynamic_slot(0));
    let cases: &[(u8, &str, i64, Option<&str>)] = &[
        (3, "pool full", -32005, Some("objectPoolFull")),
        (4, "stale handle", -32005, Some("unknownSlot")),
        (5, "entity window", -32005, Some("slotOwnedByEntityWindow")),
        (2, "bad archetype", -32602, None),
        (1, "bad op — ours, not the caller's", -32603, None),
    ];
    for (status, what, want_code, want_reason) in cases {
        script(&mut f.c, *status, 0);
        let v = f.c.call(
            "emulator/object_move",
            json!({"handle": h, "x": 1111, "y": 2222}),
        );
        assert!(
            v.get("result").is_none(),
            "status {status} ({what}) came back as a RESULT: {v}"
        );
        let (code, _, data) = err_of(&v["error"]);
        assert_eq!(code, *want_code, "status {status} ({what})");
        match want_reason {
            Some(r) => assert_eq!(data["reason"], json!(r), "status {status}"),
            None => assert!(data.get("reason").is_none(), "status {status}"),
        }
        assert_eq!(
            data["framesAdvanced"],
            json!(1),
            "status {status}: rule (5) puts framesAdvanced on a FAILURE too"
        );
    }
}

/// A request the engine never acknowledges is `mailboxNotConsumed` — **never a success** — and the
/// server CANCELS it so it cannot fire minutes later, when the game happens to enter the state that
/// carries the consumer.
#[test]
fn an_unconsumed_request_is_refused_and_cancelled() {
    let mut f = fixture("deaf");
    poke(&mut f.c, DEAF, 1, 1);
    let (code, message, data) = err_of(&f.c.err(
        "emulator/object_spawn",
        json!({"def": "0x00000200", "x": 1, "y": 1, "maxFrames": 3}),
    ));
    assert_eq!(code, -32005);
    assert_eq!(data["reason"], json!("mailboxNotConsumed"));
    assert_eq!(data["cancelled"], json!(true));
    assert_eq!(data["framesAdvanced"], json!(3), "the whole budget ran");
    assert!(
        message.contains("not in a state that services this mailbox"),
        "the commonest error a user will meet must not read as a broken feature: {message}"
    );
    assert_eq!(
        peek(&mut f.c, MB_FLAG, 1)[0],
        0,
        "the request was left ARMED after its own error reply"
    );
    // `Obj_Req_Op` is deliberately left alone, so a watchpoint can still see what the last request was.
    assert_eq!(
        peek(&mut f.c, MB_OP, 1)[0],
        1,
        "the op byte was cleared too"
    );
}

// ---------------------------------------------------------------------------------------------------
// 3. The protocol the server hides
// ---------------------------------------------------------------------------------------------------

/// **Every cell of the payload is this request's, witnessed by the consumer rather than by reading our
/// own source** — no cell is left carrying the previous request's value for the engine to act on.
///
/// ⚑ **This test does NOT witness flag-last, and the attempt to make it do so is the finding.** The
/// obvious claim — *"write the flag before the payload and this goes red"* — was written, mutated on
/// disk, and came back **green**: with the machine paused, no CPU cycles elapse between the server's
/// seven writes, so the consumer cannot observe any order among them. Which is *exactly* CR-J §5.1(a),
/// arriving as a measurement instead of an argument: flag-last over a request/response bus is safe on
/// this server *"only through an unstated property of `require_paused`"*, and a rule whose correctness
/// depends on an unstated server property is worse than no rule. The server writes the flag last
/// regardless — the contract says so, it costs nothing, and a server whose machine ticks between
/// writes would need it — but nothing in this suite can hold it, and a test named as if it could would
/// be a control that measures a different string from the one it names.
///
/// What is left is real and does bite: drop or reorder *which cells get written* and the consumer sees
/// a stale value, which the second request below is built to catch in every cell at once.
#[test]
fn every_payload_cell_the_consumer_reads_belongs_to_this_request() {
    let mut f = fixture("flaglast");
    seat_record(&mut f.c, dynamic_slot(3), 7, 9);
    script(&mut f.c, 0, slot_addr(dynamic_slot(3)) & 0xFFFF);

    f.c.ok(
        "emulator/object_spawn",
        json!({"def": "0x00000200", "x": 111, "y": 222, "subtype": 0x11}),
    );
    assert_eq!(
        u16::from_be_bytes([peek(&mut f.c, W_X, 2)[0], peek(&mut f.c, W_X, 2)[1]]),
        111
    );

    // A second, wholly different request: every witnessed cell must be the NEW one.
    f.c.ok(
        "emulator/object_spawn",
        json!({"def": "0x00000208", "x": 4444, "y": 3333, "subtype": 0x22, "flipH": true, "flipV": true}),
    );
    let w = |c: &mut Client, a: u32| {
        let b = peek(c, a, 2);
        u16::from_be_bytes([b[0], b[1]])
    };
    assert_eq!(w(&mut f.c, W_X), 4444, "X was stale when the flag went up");
    assert_eq!(w(&mut f.c, W_Y), 3333, "Y was stale when the flag went up");
    assert_eq!(w(&mut f.c, W_PLACE), 0x6022, "the placement word was stale");
    assert_eq!(
        peek(&mut f.c, W_DEF, 4),
        vec![0x00, 0x00, 0x02, 0x08],
        "the archetype pointer was stale"
    );
    assert_eq!(peek(&mut f.c, W_OP, 1)[0], 1, "the op byte was stale");
}

/// The placement word is **composed server-side and total**: subtype in the low byte, the two flips in
/// bits 13 and 14, and no other bit set — so nothing a caller sent can vanish into the engine's
/// `$60FF` mask unremarked. Read out of the consumer's witness, not out of our own composer.
#[test]
fn the_placement_word_is_composed_from_the_structured_params() {
    let mut f = fixture("place");
    seat_record(&mut f.c, dynamic_slot(4), 1, 1);
    script(&mut f.c, 0, slot_addr(dynamic_slot(4)) & 0xFFFF);
    for (subtype, h, v, want) in [
        (0u64, false, false, 0x0000u16),
        (0x3C, false, false, 0x003C),
        (0x3C, true, false, 0x203C),
        (0x3C, false, true, 0x403C),
        (0xFF, true, true, 0x60FF),
    ] {
        f.c.ok(
            "emulator/object_spawn",
            json!({"def": "0x00000200", "x": 1, "y": 1, "subtype": subtype, "flipH": h, "flipV": v}),
        );
        let b = peek(&mut f.c, W_PLACE, 2);
        assert_eq!(u16::from_be_bytes([b[0], b[1]]), want, "subtype {subtype}");
    }
}

/// The three ops are the server's own bytes and the client never spells one. Each row writes its own.
#[test]
fn each_row_writes_its_own_op_byte_and_the_client_never_sees_one() {
    let mut f = fixture("ops");
    seat_record(&mut f.c, dynamic_slot(5), 1, 1);
    let h = handle_of(dynamic_slot(5));
    script(&mut f.c, 0, slot_addr(dynamic_slot(5)) & 0xFFFF);
    for (method, params, op) in [
        (
            "emulator/object_spawn",
            json!({"def": "0x00000200", "x": 1, "y": 1}),
            1u8,
        ),
        (
            "emulator/object_move",
            json!({"handle": h, "x": 1, "y": 1}),
            2,
        ),
        ("emulator/object_delete", json!({"handle": h}), 3),
    ] {
        f.c.ok(method, params);
        assert_eq!(peek(&mut f.c, W_OP, 1)[0], op, "{method}");
    }
}

// ---------------------------------------------------------------------------------------------------
// 4. The reply
// ---------------------------------------------------------------------------------------------------

/// **`x`/`y` RE-READ the record after the advance; they never echo the request** (§11.32's 2026-09-03
/// addendum).
///
/// The double writes no position at all, so the record still holds what this test seated there. A
/// server that echoed would answer `(1111, 2222)`; the machine says `(7, 9)`, and only the re-read can
/// produce it. This is the assertion that could not be made against a real ROM, where a stationary
/// object makes the two readings agree.
#[test]
fn the_reply_re_reads_the_record_and_does_not_echo_the_request() {
    let mut f = fixture("reread");
    let slot = dynamic_slot(6);
    seat_record(&mut f.c, slot, 7, 9);
    script(&mut f.c, 0, slot_addr(slot) & 0xFFFF);

    let r = f.c.ok(
        "emulator/object_move",
        json!({"handle": handle_of(slot), "x": 1111, "y": 2222}),
    );
    assert_eq!(r["x"], json!(7), "an ECHO of the accepted request: {r}");
    assert_eq!(r["y"], json!(9), "an ECHO of the accepted request: {r}");

    // …and it agrees with the other instrument on this bus, key for key.
    let s = f.c.ok("emulator/object_slot", json!({ "slot": slot }));
    assert_eq!(r["x"], s["x"]);
    assert_eq!(r["y"], s["y"]);
    assert_eq!(
        r["addr"], s["addr"],
        "the same address spelling as object_list"
    );

    let spawned = f.c.ok(
        "emulator/object_spawn",
        json!({"def": "0x00000200", "x": 3333, "y": 4444}),
    );
    assert_eq!(spawned["x"], json!(7), "spawn echoed too: {spawned}");
    assert_eq!(spawned["y"], json!(9));
}

/// `handle` is the low word of `addr`, `slot` is the join back to `object_list`, and all three agree.
#[test]
fn the_handle_the_address_and_the_slot_are_one_fact_in_three_spellings() {
    let mut f = fixture("join");
    let slot = dynamic_slot(7);
    seat_record(&mut f.c, slot, 20, 30);
    script(&mut f.c, 0, slot_addr(slot) & 0xFFFF);

    let by_handle = f.c.ok(
        "emulator/object_move",
        json!({"handle": handle_of(slot), "x": 1, "y": 1}),
    );
    let by_slot = f.c.ok(
        "emulator/object_move",
        json!({"slot": slot, "x": 1, "y": 1}),
    );
    assert_eq!(
        by_handle["handle"], by_slot["handle"],
        "two spellings, one object"
    );
    assert_eq!(by_handle["addr"], by_slot["addr"]);
    assert_eq!(by_handle["slot"], json!(slot));
    let addr = u32::from_str_radix(&by_handle["addr"].as_str().unwrap()[2..], 16).unwrap();
    assert_eq!(
        by_handle["handle"],
        json!(format!("0x{:04X}", addr & 0xFFFF)),
        "the contract states the arithmetic; the reply must satisfy it"
    );
    assert_eq!(addr, slot_addr(slot));
}

/// `framesAdvanced` is on every reply, the machine is **paused before and paused after**, and the
/// advance is a change of position rather than of mode.
#[test]
fn the_machine_is_paused_before_and_after_and_the_advance_is_reported() {
    let mut f = fixture("frames");
    seat_record(&mut f.c, dynamic_slot(8), 1, 1);
    script(&mut f.c, 0, slot_addr(dynamic_slot(8)) & 0xFFFF);
    let before = f.c.ok("emulator/status", json!({}));
    let r = f.c.ok(
        "emulator/object_move",
        json!({"handle": handle_of(dynamic_slot(8)), "x": 1, "y": 1}),
    );
    let advanced = r["framesAdvanced"].as_u64().expect("framesAdvanced");
    assert!(
        advanced >= 1,
        "the ack cannot be collected without advancing"
    );
    assert_eq!(
        r["running"],
        json!(false),
        "the machine must be paused after"
    );
    assert_eq!(before["running"], json!(false));
    assert_eq!(
        r["frame"].as_u64().unwrap(),
        before["frame"].as_u64().unwrap() + advanced,
        "the stamp must move by exactly what framesAdvanced says"
    );
}

/// `emulator/object_delete` reports no `deleted: true` — a field true on every success carries no
/// information — and no position, because a delete names a thing rather than a place.
#[test]
fn delete_reports_what_it_acted_on_and_nothing_that_could_never_be_false() {
    let mut f = fixture("del");
    let slot = dynamic_slot(9);
    seat_record(&mut f.c, slot, 1, 1);
    script(&mut f.c, 0, slot_addr(slot) & 0xFFFF);
    let r =
        f.c.ok("emulator/object_delete", json!({"handle": handle_of(slot)}));
    assert!(r.get("deleted").is_none(), "`deleted` is always true: {r}");
    assert!(r.get("x").is_none(), "a delete names a thing, not a place");
    assert!(r.get("y").is_none());
    assert_eq!(r["handle"], json!(handle_of(slot)));
    assert!(r.get("framesAdvanced").is_some());
    assert!(r.get("layout").is_some(), "the ⚙ group's rule (1)");
}

// ---------------------------------------------------------------------------------------------------
// 5. Refusals that are the server's
// ---------------------------------------------------------------------------------------------------

/// §6's run-control state rule: all three rows need a paused machine, and none of them pauses one
/// implicitly.
#[test]
fn all_three_rows_refuse_a_free_running_machine() {
    let mut f = fixture("running");
    f.c.ok("emulator/resume", json!({}));
    for (method, params) in [
        (
            "emulator/object_spawn",
            json!({"def": "0x00000200", "x": 1, "y": 1}),
        ),
        (
            "emulator/object_move",
            json!({"handle": handle_of(dynamic_slot(0)), "x": 1, "y": 1}),
        ),
        (
            "emulator/object_delete",
            json!({"handle": handle_of(dynamic_slot(0))}),
        ),
    ] {
        let (code, _, data) = err_of(&f.c.err(method, params));
        assert_eq!(code, -32005, "{method}");
        assert_eq!(data["reason"], json!("machineRunning"), "{method}");
        assert_eq!(
            data["framesAdvanced"],
            json!(0),
            "{method}: rule (5)'s framesAdvanced on a refusal too — 0 is the answer"
        );
    }
    let st = f.c.ok("emulator/status", json!({}));
    assert_eq!(
        st["running"],
        json!(true),
        "a refused row PAUSED the machine — the implicit state change §5 forbids"
    );
}

/// `expectFrameToken` refuses a machine that moved under the caller, and does it **before** writing.
#[test]
fn a_stale_frame_token_is_refused_before_anything_is_written() {
    let mut f = fixture("token");
    seat_record(&mut f.c, dynamic_slot(10), 1, 1);
    script(&mut f.c, 0, slot_addr(dynamic_slot(10)) & 0xFFFF);
    let now = f.c.ok("emulator/status", json!({}))["frame"]
        .as_u64()
        .unwrap();

    // The matching token is accepted…
    f.c.ok(
        "emulator/object_move",
        json!({"handle": handle_of(dynamic_slot(10)), "x": 1, "y": 1, "expectFrameToken": now}),
    );
    // …and a token from before the advance that call made is not.
    let hash_before = f.c.ok("emulator/memory_hash", ram_window())["hash"].clone();
    let (code, _, data) = err_of(&f.c.err(
        "emulator/object_move",
        json!({"handle": handle_of(dynamic_slot(10)), "x": 5, "y": 5, "expectFrameToken": now}),
    ));
    assert_eq!(code, -32005);
    assert_eq!(data["reason"], json!("frameMoved"));
    assert_eq!(data["framesAdvanced"], json!(0));
    assert_eq!(
        f.c.ok("emulator/memory_hash", ram_window())["hash"],
        hash_before,
        "a frameMoved refusal wrote to the machine anyway"
    );
}

/// §8.4's layout assertion, against a ROM this server was handed: every name resolves, every width is
/// right, and the flag is **not** the last cell. Nothing above the assertion would notice.
#[test]
fn a_mailbox_whose_flag_is_not_the_last_cell_is_refused() {
    let mut f = fixture_without_mailbox("layout");
    let mut rows = pool_rows();
    // Somebody's new u16 declared just before the flag: the block still starts where it did, every name
    // still resolves, and the ack is no longer the last write of a consumption.
    rows.extend(vec![
        ("Obj_Req_Def".into(), MB_DEF),
        ("Obj_Req_X".into(), MB_X),
        ("Obj_Req_Y".into(), MB_Y),
        ("Obj_Req_Slot".into(), MB_SLOT),
        ("Obj_Req_Place".into(), MB_PLACE),
        ("Obj_Req_Op".into(), MB_OP),
        ("Obj_Req_Status".into(), MB_STATUS),
        ("Obj_Req_Flag".into(), MB_FLAG + 2),
    ]);
    load_listing(&mut f.c, "layoutbad", &rows);
    let hash_before = f.c.ok("emulator/memory_hash", ram_window())["hash"].clone();
    let (code, _, data) = err_of(&f.c.err(
        "emulator/object_spawn",
        json!({"def": "0x00000200", "x": 1, "y": 1}),
    ));
    assert_eq!(code, -32005);
    assert_eq!(data["reason"], json!("mailboxLayoutUnexpected"));
    assert_eq!(
        data["resolved"].as_array().map(Vec::len),
        Some(8),
        "the refusal names the layout it refused"
    );
    assert_eq!(
        f.c.ok("emulator/memory_hash", ram_window())["hash"],
        hash_before,
        "a layout refusal wrote to the machine anyway"
    );
}

/// The pre-flight §11.32 grants: a slot outside the dynamic pool is refused with a message that says
/// what the row reaches, rather than the engine's `unknownSlot` on a perfectly real player.
#[test]
fn a_slot_outside_the_dynamic_pool_is_refused_before_a_frame_is_burned() {
    let mut f = fixture("preflight");
    for slot in [0u32, 1, NUM_PLAYERS + NUM_DYNAMIC, NUM_TOTAL - 1] {
        let (code, message, data) = err_of(&f.c.err(
            "emulator/object_move",
            json!({"slot": slot, "x": 1, "y": 1}),
        ));
        assert_eq!(code, -32602, "slot {slot}");
        assert_eq!(data["framesAdvanced"], json!(0), "slot {slot}: nothing ran");
        assert!(
            message.contains("DYNAMIC"),
            "slot {slot}: the message must name what the row reaches: {message}"
        );
    }
    // …and a dynamic slot passes the pre-flight and reaches the engine.
    script(&mut f.c, 0, slot_addr(dynamic_slot(0)) & 0xFFFF);
    seat_record(&mut f.c, dynamic_slot(0), 1, 1);
    f.c.ok(
        "emulator/object_move",
        json!({"slot": dynamic_slot(0), "x": 1, "y": 1}),
    );
}

/// `def` outside the cart window is refused **before any write**: a pointer that cannot be an archetype
/// never reaches the machine.
#[test]
fn an_archetype_pointer_outside_the_cart_window_never_reaches_the_machine() {
    let mut f = fixture("cart");
    let hash_before = f.c.ok("emulator/memory_hash", ram_window())["hash"].clone();
    let (code, _, data) = err_of(&f.c.err(
        "emulator/object_spawn",
        json!({"def": "0x00400000", "x": 1, "y": 1}),
    ));
    assert_eq!(code, -32602);
    assert_eq!(data["def"], json!("0x00400000"));
    assert_eq!(data["framesAdvanced"], json!(0));
    assert_eq!(
        f.c.ok("emulator/memory_hash", ram_window())["hash"],
        hash_before
    );
    // The bound is EXCLUSIVE, and the rail split §11.32 Q1 describes is visible in the reply: the last
    // address inside the window passes the pre-flight and reaches the engine, whose own refusal comes
    // back as the same code with the four rails named and a frame burned.
    script(&mut f.c, 2, 0);
    let (code, _, data) = err_of(&f.c.err(
        "emulator/object_spawn",
        json!({"def": "0x003FFFFE", "x": 1, "y": 1}),
    ));
    assert_eq!(code, -32602);
    assert_eq!(data["def"], json!("0x003FFFFE"));
    assert_eq!(
        data["rails"].as_array().map(Vec::len),
        Some(4),
        "the engine collapses four rails into one byte, so the refusal names all four"
    );
    assert_eq!(
        data["framesAdvanced"],
        json!(1),
        "this one reached the machine — which is what makes it a different refusal from the pre-flight"
    );
}

/// The exactly-one-of pairs, refused in **both** directions on all three rows.
#[test]
fn the_exactly_one_of_pairs_are_enforced_in_both_directions() {
    let mut f = fixture("oneof");
    let h = handle_of(dynamic_slot(0));
    for (method, params) in [
        (
            "emulator/object_spawn",
            json!({"def": "0x00000200", "defSymbol": "Player_1", "x": 1, "y": 1}),
        ),
        ("emulator/object_spawn", json!({"x": 1, "y": 1})),
        (
            "emulator/object_move",
            json!({"handle": h, "slot": 3, "x": 1, "y": 1}),
        ),
        ("emulator/object_move", json!({"x": 1, "y": 1})),
        ("emulator/object_delete", json!({"handle": h, "slot": 3})),
        ("emulator/object_delete", json!({})),
    ] {
        let (code, _, _) = err_of(&f.c.err(method, params.clone()));
        assert_eq!(code, -32602, "{method} {params}");
    }
}

/// `defSymbol` is resolved per call, and a name the table does not hold is `-32013` rather than a
/// guessed address.
#[test]
fn def_symbol_resolves_per_call_and_refuses_a_name_it_does_not_hold() {
    let mut f = fixture("defsym");
    seat_record(&mut f.c, dynamic_slot(11), 1, 1);
    script(&mut f.c, 0, slot_addr(dynamic_slot(11)) & 0xFFFF);
    // `ObjCodeBase` is in the listing at an even, in-window address, so it passes the pre-flight and
    // reaches the double exactly as an address would.
    f.c.ok(
        "emulator/object_spawn",
        json!({"defSymbol": "ObjCodeBase", "x": 1, "y": 1}),
    );
    assert_eq!(
        peek(&mut f.c, W_DEF, 4),
        vec![0x00, 0x01, 0x00, 0x00],
        "the symbol was not resolved to its address"
    );
    let (code, _, _) = err_of(&f.c.err(
        "emulator/object_spawn",
        json!({"defSymbol": "ObjDef_NoSuchThing", "x": 1, "y": 1}),
    ));
    assert_eq!(code, -32013);
}

/// Bounds are refused, never clamped — including the one that pays for the structured placement params.
#[test]
fn out_of_range_params_are_refused_and_never_clamped() {
    let mut f = fixture("bounds");
    for params in [
        json!({"def": "0x00000200", "x": 65536, "y": 1}),
        json!({"def": "0x00000200", "x": 1, "y": 65536}),
        json!({"def": "0x00000200", "x": -1, "y": 1}),
        json!({"def": "0x00000200", "x": 1, "y": 1, "subtype": 256}),
        json!({"def": "0x00000200", "x": 1, "y": 1, "maxFrames": 0}),
        json!({"def": "0x00000200", "x": 1, "y": 1, "flipH": "yes"}),
    ] {
        let (code, _, _) = err_of(&f.c.err("emulator/object_spawn", params.clone()));
        assert_eq!(code, -32602, "{params}");
    }
}

// ---------------------------------------------------------------------------------------------------
// 6. Servedness
// ---------------------------------------------------------------------------------------------------

/// **Three rows, not one `object_request { op }`** — servedness on this bus is `methods` membership, so
/// *can spawn* and *can delete* must be able to be different bits.
#[test]
fn the_three_rows_are_three_separate_names_in_methods() {
    let h = spawn_with("advertise", consumer_rom(), 8);
    let mut c = Client::connect(&h);
    let init = c.handshake(false);
    let methods: Vec<&str> = init["methods"]
        .as_array()
        .expect("methods")
        .iter()
        .map(|m| m.as_str().unwrap())
        .collect();
    for row in [
        "emulator/object_spawn",
        "emulator/object_move",
        "emulator/object_delete",
    ] {
        assert!(methods.contains(&row), "{row} is not advertised");
    }
    assert!(
        !methods.contains(&"emulator/object_request"),
        "a single op-switched row would make `can spawn` and `can delete` the same bit"
    );
    assert_eq!(
        init["capabilities"]["objectDecoders"],
        json!(true),
        "true iff at least one of the ⚙ rows is in `methods` — never whether a layout was detected"
    );
}

/// The rows are advertised **unconditionally**, and refuse at call time. `load_symbols` may be called
/// at any point in a session, so a handshake-time answer would be stale by construction.
#[test]
fn the_rows_are_advertised_even_where_no_symbol_table_is_loaded() {
    let h = spawn_with("nosyms", consumer_rom(), 8);
    let mut c = Client::connect(&h);
    let init = c.handshake(false);
    assert!(init["methods"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m == "emulator/object_spawn"));
    c.ok("emulator/pause", json!({}));
    let (code, _, _) = err_of(&c.err(
        "emulator/object_spawn",
        json!({"def": "0x00000200", "x": 1, "y": 1}),
    ));
    assert_eq!(
        code, -32012,
        "no table at all is a different answer from a release ROM"
    );
}
