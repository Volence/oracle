//! ⚑ **The tripwire for the migration's S0**, and it is a *structural* assertion before it is a numeric one.
//!
//! `pick.rs`'s `#[cfg(feature = "aether")] mod bus_parity` is the strongest correctness guarantee in
//! `oracle-frontend`: it asserts address-level agreement between the picking panel and
//! `emulator/pixel_attribution` for every dot of four sprite shapes under all four flips, plus the mask rows
//! and §11.27's colour caveat. It can only exist in a compilation unit that can see **both** the panel and
//! `oracle_aether::engine::Engine`.
//!
//! `docs/2026-09-05-frontend-migration-recon.md` §3.0(b) names the way that guarantee dies quietly: **a lib
//! crate carved out of `oracle-frontend` without the `oracle-aether` dependency edge would delete it and
//! stay green.** S0 avoided that by making the lib target a `lib.rs` *inside this crate* rather than a new
//! crate, so the edge is the crate's own. This file is what makes that a checked fact rather than a note:
//! it lives in `tests/`, so it links the **lib target**, and it names both sides in one unit. If a later
//! slice ever moves `pick` into a crate that cannot see `oracle-aether`, this stops compiling — which is
//! the loud failure the silent one was going to be.
//!
//! It is deliberately one dot rather than a second copy of `bus_parity`. Duplicating the sprite sweep here
//! would be two implementations of one claim, drifting; the claim under test in *this* file is that the two
//! sides are reachable at once, and one address-level row is enough to prove the reach is real rather than
//! a `use` that the compiler kept.

#![cfg(feature = "aether")]

use oracle_aether::engine::{Engine, EngineConfig};
use oracle_aether::outbound::Subscribers;
use oracle_core::render::LayerMask;
use oracle_core::system::System;
use oracle_core::vdp::Vdp;
use oracle_frontend::pick::{resolve, Space};
use serde_json::json;

/// The machine's now, parked two frames in. §11.27's colour caveat is a comparison against the machine's
/// clock, and a freshly reset machine has **no completed frame** — so a fixture left where `reset` put it
/// would be measuring a disclosure rather than the address agreement this row is named for. The same
/// constant, for the same reason, as `pick.rs`'s own `QUIET_NOW`.
const QUIET_NOW: u64 = 2 * oracle_core::vdp::MCLK_PER_FRAME;

fn set_reg(v: &mut Vdp, reg: u8, val: u8) {
    v.control_write(0x8000 | (u16::from(reg) << 8) | u16::from(val), 0);
}

fn write_vram(v: &mut Vdp, addr: u16, words: &[u16]) {
    v.control_write(0x4000 | (addr & 0x3FFF), 0);
    v.control_write(addr >> 14, 0);
    for w in words {
        v.data_write(*w);
    }
}

/// An engine whose VDP is **byte-identical** to the one the panel is handed. Cloned rather than rebuilt:
/// the whole claim is that two readers of one state agree, and two states built twice would let the test
/// keep them in step itself.
fn engine_showing(v: &Vdp) -> Engine {
    let mut sys = System::new(0x5EED);
    sys.load_rom(oracle_core::testrom::build());
    sys.reset();
    *sys.vdp_mut() = v.clone();
    let now = sys.scheduler().now();
    assert!(
        now <= QUIET_NOW,
        "reset already left the clock past QUIET_NOW"
    );
    sys.scheduler_mut().advance(QUIET_NOW - now);
    Engine::new(sys, EngineConfig::default(), Subscribers::new())
}

#[test]
fn the_lib_target_can_see_the_panel_and_the_engine_at_once() {
    // One opaque plane-A cell at the top-left, and a non-zero backdrop everywhere else. Registers are the
    // same ones `pick.rs`'s own plane row sets, because they are the minimum that makes a dot resolvable.
    let mut rng = oracle_core::rng::SplitMix64::new(0x5EED);
    let mut v = Vdp::power_on(&mut rng);
    v.vram_mut().fill(0);
    set_reg(&mut v, 0x01, 0x74); // display on, mode 5
    set_reg(&mut v, 0x0C, 0x81); // H40
    set_reg(&mut v, 0x02, 0x30); // plane A nametable @ $C000
    set_reg(&mut v, 0x04, 0x07); // plane B nametable @ $E000
    set_reg(&mut v, 0x05, 0x58); // SAT @ $B000, empty
    set_reg(&mut v, 0x07, 0x25); // backdrop = CRAM entry $25
    set_reg(&mut v, 0x0F, 0x02);
    set_reg(&mut v, 0x10, 0x00);
    write_vram(&mut v, 0xC000, &[(1 << 13) | 0x055]);
    write_vram(&mut v, 0x055 * 32, &[0x3333; 16]);

    let mut e = engine_showing(&v);
    let now = e.stamp()["mclk"]
        .as_u64()
        .expect("the §2.2 stamp carries mclk");

    // --- The panel's answer, in-process, and the bus's answer, dispatched. One dot, two readers. ---
    let wire = e
        .dispatch("emulator/pixel_attribution", &json!({"x": 2, "y": 2}))
        .expect("a dot inside the active display must answer");
    let panel = resolve(&v, 2, 2, LayerMask::ALL, now);

    assert_eq!(wire["winner"]["layer"], json!("planeA"));
    assert_eq!(panel.targets[0].space, Space::Vram);
    // The bus spells addresses as hex strings (D9 category 1); the panel carries numbers. The comparison
    // has to cross that boundary explicitly, and the expectation is read off the wire reply rather than
    // written here as a literal — a literal would be this test agreeing with itself.
    let wire_addr = u32::from_str_radix(
        wire["cell"]["tileAddr"]
            .as_str()
            .expect("an address string")
            .trim_start_matches("0x"),
        16,
    )
    .expect("hex");
    assert_eq!(
        panel.targets[0].lo, wire_addr,
        "the panel arms ${:04X} and the bus names {} — the two have DRIFTED, and the lib target is where \
         that is catchable",
        panel.targets[0].lo, wire["cell"]["tileAddr"]
    );
    assert_eq!(panel.targets[0].hi, wire_addr + 31, "a 32-byte pattern");
}
