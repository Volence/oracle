//! **Contract §11.51 on the wire** (`docs/2026-09-25-bank-blind-wire-check.md`): cartridge addresses are
//! bus addresses and bank-blind, and a client reconstructs the bank from a `write` watch on the mapper
//! registers.
//!
//! One hand-assembled program re-points four mapper windows using every write width the bus honours, then
//! reads and calls into a re-pointed window. The tests measure three things through the Aether wire:
//!
//! - a breakpoint, a `run_to` target and a bus read watch inside a re-pointed window match the **bus**
//!   address and report the bus address, while the bytes they saw are the re-pointed bank's;
//! - the reconstruction recipe works when it reads a word write's hit correctly (`addr + 1` for a word at
//!   an even address, and the low byte of `value`), and the recipe as first published (§2.4 at empyrean
//!   `7040319f`: watch `$A130F3-$A130FF`, window `(addr - $A130F1) / 2`, bank = `value`) misses or
//!   mis-assigns word writes.
//!
//! **Where the expectations come from.** The bank each window really shows is read back from the server's
//! own disclosure, `read_memory`'s `region` (§11.48), at the window's base (`k * CART_BANK_SIZE`, the core
//! constant). No expected bank is typed into an assertion. The recipe under test is the contract's formula,
//! written out here: the ruler is not the mapper's own `window_of_register`.

mod common;

use common::{resume_and_wait_for_stop, spawn_system, Client};
use oracle_aether::server::ServerHandle;
use oracle_core::bus::{CART_BANK_SIZE, CART_WINDOWS};
use oracle_core::system::System;
use serde_json::{json, Value};

/// Ten banks, so every bank the program selects exists in the image.
const BANKS: usize = 10;
/// The contract's register base: window `n`'s register is `$A130F1 + 2n` (§2.4 as published).
const REG_BASE: u32 = 0xA1_30F1;
/// Where the program calls into window 1 once it shows bank 8.
const CALL_TARGET: u32 = 0x08_0010;
/// The bank the program's byte write gives window 1.
const CALL_BANK: usize = 8;

fn put_w(rom: &mut [u8], at: usize, w: u16) {
    rom[at] = (w >> 8) as u8;
    rom[at + 1] = w as u8;
}

/// The image: every bank's first byte is `$B0 | bank`, bank 8 holds an `rts` at the call target's offset
/// and bank 1 (the identity bank for window 1) holds a `nop` there, so the fetched word names its bank.
fn image() -> Vec<u8> {
    let mut rom = vec![0u8; BANKS * CART_BANK_SIZE];
    for b in 0..BANKS {
        rom[b * CART_BANK_SIZE] = 0xB0 | b as u8;
    }
    put_w(&mut rom, 0, 0x00FF);
    put_w(&mut rom, 2, 0xFFFE); // SSP
    put_w(&mut rom, 4, 0x0000);
    put_w(&mut rom, 6, 0x0200); // PC
    let program: &[u16] = &[
        0x46FC, 0x2700, // move.w #$2700,sr
        0x33FC, 0x0007, 0x00A1, 0x30F2, // move.w #7,($A130F2).l        word, window 1 -> 7
        0x13FC, 0x0008, 0x00A1, 0x30F3, // move.b #8,($A130F3).l        byte, window 1 -> 8
        0x33FC, 0x0109, 0x00A1,
        0x30F4, // move.w #$0109,($A130F4).l    word, window 2 -> 9 (low byte)
        0x23FC, 0x0005, 0x0006, 0x00A1,
        0x30F6, // move.l #$00050006,($A130F6).l  windows 3,4 -> 5,6
        0x1039, 0x0008, 0x0000, // move.b ($080000).l,d0
        0x4EB9, 0x0008, 0x0010, // jsr ($080010).l
        0x60FE, // bra.s *
    ];
    for (i, &w) in program.iter().enumerate() {
        put_w(&mut rom, 0x200 + 2 * i, w);
    }
    let off = CALL_TARGET as usize & (CART_BANK_SIZE - 1);
    put_w(&mut rom, CALL_BANK * CART_BANK_SIZE + off, 0x4E75); // rts
    put_w(&mut rom, CART_BANK_SIZE + off, 0x4E71); // nop
    rom
}

fn server(tag: &str) -> (ServerHandle, Client) {
    let mut sys = System::new(0xB4B);
    sys.load_rom(image());
    sys.reset();
    let h = spawn_system(tag, sys, 1024);
    let mut c = Client::connect(&h);
    c.handshake(true);
    (h, c)
}

fn hex(v: &Value) -> u32 {
    u32::from_str_radix(
        v.as_str().expect("a hex string").trim_start_matches("0x"),
        16,
    )
    .unwrap()
}

fn addr(a: u32) -> String {
    format!("0x{a:06X}")
}

/// The bank window `k` shows now, as the server discloses it: `"cartridge ROM"` is the identity (bank
/// `k`), `"cartridge ROM bank N"` is bank N.
fn disclosed_bank(c: &mut Client, k: usize) -> usize {
    let r = c.ok(
        "emulator/read_memory",
        json!({"addr": addr((k * CART_BANK_SIZE) as u32), "len": 1}),
    );
    let region = r["region"].as_str().expect("a bus read carries region");
    match region.strip_prefix("cartridge ROM bank ") {
        Some(n) => n.parse().unwrap(),
        None => {
            assert_eq!(region, "cartridge ROM", "window {k}: unexpected region");
            k
        }
    }
}

fn hits(c: &mut Client, watch: &Value) -> Vec<Value> {
    let r = c.ok("emulator/watchpoint_hits", json!({"watch": watch}));
    assert_eq!(r["truncated"], json!(false));
    assert_eq!(r["dropped"], json!(0));
    r["hits"].as_array().unwrap().clone()
}

/// Run the program to its call into window 1 (a `run_to` on a bus address the identity bank would also
/// have), and return the reply.
fn run_to_call(c: &mut Client) -> Value {
    let r = c.ok("emulator/run_to", json!({"addr": addr(CALL_TARGET)}));
    assert_eq!(r["reached"], json!(true), "{r}");
    r
}

/// The recipe, read correctly: a byte hit on an odd register, or a word hit on an even address (its low
/// byte lands on `addr + 1`), sets window `(reg - $A130F1) / 2` to the low byte of `value`. Every hit
/// carries `mclk`, and the hits are in `mclk` order.
#[test]
fn the_register_watch_reconstructs_every_window_when_word_hits_are_read_at_addr_plus_one() {
    let (_h, mut c) = server("bbw-recipe");
    let w = c.ok(
        "emulator/watchpoint_add",
        json!({"addr": addr(REG_BASE + 1), "len": 14, "write": true}),
    )["watch"]
        .clone();
    let stop = run_to_call(&mut c);
    let hs = hits(&mut c, &w);
    assert!(hs.len() >= 4, "the program writes the registers: {hs:?}");

    let mut rebuilt: Vec<usize> = (0..CART_WINDOWS).collect();
    let mut last_mclk = 0u64;
    for h in &hs {
        let mclk = h["mclk"].as_u64().expect("every hit carries mclk");
        assert!(mclk >= last_mclk, "hits come in mclk order");
        assert!(
            mclk < stop["mclk"].as_u64().unwrap(),
            "and before the record they explain"
        );
        last_mclk = mclk;
        let (a, size, value) = (
            hex(&h["addr"]),
            h["size"].as_u64().unwrap(),
            hex(&h["value"]),
        );
        let reg = match (size, a & 1) {
            (1, 1) => a,
            (2, 0) => a + 1,
            _ => continue, // a byte on an even address sets nothing
        };
        let n = (reg - REG_BASE) / 2;
        if (1..CART_WINDOWS as u32).contains(&n) {
            rebuilt[n as usize] = (value & 0xFF) as usize;
        }
    }
    let truth: Vec<usize> = (0..CART_WINDOWS)
        .map(|k| disclosed_bank(&mut c, k))
        .collect();
    assert_ne!(
        truth,
        (0..CART_WINDOWS).collect::<Vec<_>>(),
        "the program re-pointed nothing: this test would be vacuous"
    );
    assert_eq!(
        rebuilt, truth,
        "reconstructed banks vs the server's own region"
    );
}

/// The recipe as first published misses the word write to `$A130F2` entirely (outside `$A130F3-$A130FF`)
/// and gives every other word hit a half-integer window, and `value` is the whole word, not the bank.
#[test]
fn the_recipe_as_first_published_mis_reads_word_writes() {
    let (_h, mut c) = server("bbw-recipe-v1");
    let w = c.ok(
        "emulator/watchpoint_add",
        json!({"addr": addr(REG_BASE + 2), "len": 13, "write": true}),
    )["watch"]
        .clone();
    run_to_call(&mut c);
    let hs = hits(&mut c, &w);
    let word_hits: Vec<&Value> = hs.iter().filter(|h| h["size"] == json!(2)).collect();
    assert!(!word_hits.is_empty(), "the program makes word writes");
    for h in &word_hits {
        let a = hex(&h["addr"]);
        assert_eq!(
            (a - REG_BASE) % 2,
            1,
            "{a:#X}: a word hit sits on the even address, so (addr - $A130F1) / 2 is not a window"
        );
    }
    assert!(
        word_hits.iter().any(|h| hex(&h["value"]) > 0xFF),
        "a word's high byte rides in `value`: `value` is not the bank"
    );
    assert!(
        hs.iter().all(|h| hex(&h["addr"]) != REG_BASE + 1),
        "the $A130F2 word write is outside the published range and is not seen"
    );
    // ...yet it did re-point window 1 before the byte write replaced it: the watch saw one window-1 write
    // (the byte), while the program made two. Proven by the wider watch in the test above; here, by count.
    let window1: Vec<&Value> = hs
        .iter()
        .filter(|h| hex(&h["addr"]) == REG_BASE + 2)
        .collect();
    assert_eq!(
        window1.len(),
        1,
        "only the byte write to window 1 is visible"
    );
}

/// A breakpoint and a bus read watch inside a re-pointed window fire at, and report, the bus address;
/// the bytes they saw are the re-pointed bank's, not the identity bank's.
#[test]
fn breakpoints_and_bus_watches_match_the_bus_address_in_a_re_pointed_window() {
    let (_h, mut c) = server("bbw-bankblind");
    let base = (CART_BANK_SIZE) as u32; // window 1
    let rd = c.ok(
        "emulator/watchpoint_add",
        json!({"addr": addr(base), "len": 1, "read": true}),
    )["watch"]
        .clone();
    let fetch = c.ok(
        "emulator/watchpoint_add",
        json!({"addr": addr(CALL_TARGET), "len": 2, "read": true}),
    )["watch"]
        .clone();
    c.ok(
        "emulator/breakpoint_add",
        json!({"addr": addr(CALL_TARGET)}),
    );
    let stop = resume_and_wait_for_stop(&mut c, 9001);
    assert_eq!(stop["reason"], json!("breakpoint"));
    assert_eq!(
        hex(&stop["pc"]),
        CALL_TARGET,
        "the breakpoint reports the bus address"
    );

    let img = image();
    let bank = disclosed_bank(&mut c, 1);
    assert_ne!(
        bank, 1,
        "window 1 must be re-pointed for this to mean anything"
    );
    let off = CALL_TARGET as usize & (CART_BANK_SIZE - 1);
    let banked_word = u32::from(u16::from_be_bytes([
        img[bank * CART_BANK_SIZE + off],
        img[bank * CART_BANK_SIZE + off + 1],
    ]));
    let identity_word = u32::from(u16::from_be_bytes([
        img[base as usize + off],
        img[base as usize + off + 1],
    ]));
    assert_ne!(
        banked_word, identity_word,
        "the fixture's banks must differ at the call target"
    );

    let f = hits(&mut c, &fetch);
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!(
        hex(&f[0]["addr"]),
        CALL_TARGET,
        "a fetch hit is keyed by the bus address"
    );
    assert_eq!(
        hex(&f[0]["value"]),
        banked_word,
        "and it fetched the re-pointed bank's word"
    );

    let r = hits(&mut c, &rd);
    assert_eq!(r.len(), 1, "{r:?}");
    assert_eq!(
        hex(&r[0]["addr"]),
        base,
        "a read hit is keyed by the bus address"
    );
    assert_eq!(
        hex(&r[0]["value"]),
        u32::from(img[bank * CART_BANK_SIZE]),
        "and it read the re-pointed bank's byte"
    );
    assert_ne!(img[bank * CART_BANK_SIZE], img[base as usize]);
}
