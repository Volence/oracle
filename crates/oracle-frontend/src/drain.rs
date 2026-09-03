//! **The window's whole reaction to a client changing the machine — in one function the loop and the
//! tests both call.**
//!
//! # Why this file exists
//!
//! Until `FRONTEND-LOOP-UNTESTABLE` every line below lived inline in [`crate::main`]'s windowed run loop,
//! between the frame and the present. That put it behind a `minifb::Window`, which no test in this crate
//! can construct, so **not one branch of it was witnessed by anything**. The cost was not hypothetical: on
//! 2026-09-03 the `rom_changed` branch was found — *by a person reading it*, not by a test — to re-key the
//! save-state fingerprint and say so on the glass while never touching the symbol table the window caches
//! for watchpoint symbolisation, `symbol_watch` and the lens. After an `emulator/reload_rom` that dropped
//! the listing on the D7 binding check, this window went on naming addresses out of a listing the engine
//! had discarded, while `emulator/lookup_symbol` answered out of the new one. One machine, two answers.
//!
//! The reaction now spans four report fields and the inbound pause mirror. Four fields is already more
//! than a reader can hold while asking "and does each one have a consumer?", which is exactly the question
//! that found the bug.
//!
//! # The shape, and the one it is deliberately not
//!
//! [`drain`] is modelled on `oracle_player::bus::drain`, which landed two hours earlier against the same
//! defect class in the other window. The load-bearing property is that **the pump and the reaction are one
//! call**: `bus.set_paused`, `bus.pump`, and everything the report implies happen inside [`drain`], so
//! there is no arrangement in which a test exercises the pump and misses the reaction. The rejected shape
//! is the obvious one — expose a testable `react(pumped, …)` beside the loop's own `bus.pump` — because
//! that is *precisely* the arrangement that let the reaction go missing in the first place: a caller free
//! to pump without reacting is a caller that will.
//!
//! Also rejected: routing the reaction's writes back through a returned "effects" list for the loop to
//! apply. Every effect here is a plain assignment to loop state that a `&mut` reaches, so an effects list
//! would be a second vocabulary for the same writes and a second place for one of them to go missing —
//! the same duplication in a new coat. Only the *one* effect that a `&mut` genuinely cannot reach is
//! returned, and it is named below.
//!
//! # What could not be lifted, and why
//!
//! * **The audio resynchronisation.** [`Drained::resync_audio`] is *reported*, not performed. The state it
//!   needs is `crate::AudioState`, which owns a live `cpal::Stream` and the producer half of an SPSC ring;
//!   constructing one means opening the box's real output device, which a unit test must not do. So the
//!   seam decides *whether* the timeline moved (the part that has been wrong before) and the loop performs
//!   the device work (the part that cannot be examined here). The test below asserts the flag; nothing in
//!   this crate can assert the `cpal` call, and that is recorded rather than papered over.
//! * **The terminal half of [`crate::notify`].** `notify` writes to stdout *and* to the overlay. The
//!   overlay half is observable ([`crate::overlay::Overlay::toasts`]) and is asserted; the `println!` half
//!   is captured by the test harness rather than by us, and a test that shelled out to grep its own output
//!   would be testing the harness. `bus.rs` reached the same conclusion about the same call.
//! * **The present itself.** `window.update_with_buffer` stays in the loop. This seam's output is `buf`
//!   and `width`, which is the whole of what the present consumes; the blit onto a real surface is
//!   `minifb`'s and is not a fact about the reaction.

use crate::bus::Bus;
use crate::overlay::{Overlay, ACCENT};
use crate::save_state;
use oracle_core::render::LayerMask;
use oracle_core::scanline_capture::ScanlineCapture;
use oracle_core::symbols::SymbolTable;
use oracle_core::system::System;

/// The run loop's own state that the reaction writes, borrowed for one [`drain`].
///
/// A borrow struct rather than eight positional parameters for two reasons: `clippy::too_many_arguments`
/// is right about eight, and a named field at the call site is what makes "the loop passed its `symbols`
/// and not something else" readable at the one place it matters.
pub struct Reaction<'a> {
    /// The toast/status surface. `notify` writes here; the tests read it back.
    pub ov: &'a mut Overlay,
    /// Presented-frame counter. A client-driven run advanced the machine, so the frames it ran are frames
    /// this window is accountable for.
    pub draws: &'a mut u64,
    /// The loop's scanline capture, holding lines from before a timeline jump.
    pub cap: &'a mut ScanlineCapture,
    /// The retained framebuffer the present reads.
    pub buf: &'a mut Vec<u32>,
    /// Its width. H32/H40 rides along with the picture, so the two move together or neither does.
    pub width: &'a mut usize,
    /// The save-state slot key. Every slot is written against the loaded cartridge; a cartridge that was
    /// replaced under us must re-key or a state written for the previous image loads into this one.
    pub rom_fp: &'a mut u64,
    /// **The window's own clone of the symbol listing** — `dump_hits` symbolises watchpoint PCs out of it
    /// and the lens panels name addresses from it. The field the 2026-09-03 defect left stale.
    pub symbols: &'a mut Option<SymbolTable>,
    /// The window's pause state. Written to the bus before the drain and read back after it, because
    /// `emulator/pause` is a client's way of stopping *this* loop.
    pub paused: &'a mut bool,
}

/// What one [`drain`] left for its caller: the single effect the seam declines to perform itself, and the
/// mask it painted the picture with.
///
/// It carries **no copy of the bus's own `Pumped` report**, and that absence is deliberate. Forwarding it
/// would put a field here that nothing outside a test reads — which is the precise shape of the defect
/// this file exists because of, a signal with no consumer — and it would offer a caller a second way to
/// react to the drain beside the one [`drain`] already performed. Everything the report meant is either
/// applied to [`Reaction`] or named below.
#[derive(Clone, Copy, Debug)]
pub struct Drained {
    /// **The timeline moved; audio belongs to the timeline it left.** Reported rather than performed — see
    /// the module doc. `false` on every iteration a client did not move the clock, which is nearly all of
    /// them, so the loop's audio path is untouched in the ordinary case.
    pub resync_audio: bool,
    /// **The layer mask this drain painted `buf` with**, for the status line's badge to caption the
    /// picture with. Returned rather than re-read by the caller for the reason the badge's own comment
    /// gives: a fresh read could pick up a mask a socket client set a microsecond *after* the frame was
    /// composed, and caption the picture with something that did not draw it.
    pub layers: LayerMask,
}

/// Drain the hosted bus once and apply everything the drain implies to `r`.
///
/// Bounded and non-blocking: the drain only ever `try_recv`s, every socket write happens on a connection
/// thread, and events go into per-connection queues that drop oldest-first rather than wait. A client that
/// stops reading stalls its own reader thread and nothing else. The two length bounds are the bus's —
/// `HOSTED_MAX_RUN_FRAMES` caps one command, `HostConfig::pump_budget` caps one drain.
///
/// Call position matters and the loop's comment says so: *after* the frame and its publish (so a client
/// asking for the screen this iteration gets the frame just drawn) and *before* the present (so a
/// client-driven run reaches the glass without a frame of lag).
pub fn drain(sys: &mut System, bus: &mut Bus, r: Reaction<'_>) -> Drained {
    // Conflict 1's outbound half: the window's pause state *is* the bus's free-run state.
    bus.set_paused(*r.paused);
    let pumped = bus.pump(sys);
    let mut resync_audio = false;

    if pumped.timeline_moved {
        // A client advanced (or rewound) the machine behind the loop's back. That is the same class of
        // event as a save-state load, and it needs the same two repairs: audio belongs to a timeline that
        // has moved, and the capture is holding lines from before the jump.
        *r.draws += pumped.frames_advanced;
        r.cap.clear();
        resync_audio = true;
    }
    if pumped.screen_changed {
        // The bus's advancing calls run their own scanline capture (this loop's is not attached to them),
        // so the frame they drew lives there. Pull it in; `None` means the run drew no complete frame, in
        // which case the retained image stays up exactly as it does for a 0-frame iteration.
        if let Some(w) = bus.present_frame(r.buf) {
            *r.width = w;
        }
    }
    if pumped.rom_changed {
        // `emulator/reload_rom` (or a restore that brought a different cartridge back) changed the bytes
        // under us. Re-derive the save-state fingerprint or every slot written for the previous image
        // would silently load into this one.
        *r.rom_fp = save_state::rom_fingerprint(sys.rom());
        crate::notify(
            r.ov,
            ACCENT,
            "aether: the cartridge was replaced over the bus — save-state slots re-keyed",
        );
    }
    if pumped.symbols_changed {
        // A client's `emulator/load_symbols` (or a `reload_rom` that dropped the listing on the D7 check)
        // replaced the table the engine resolves against. This window holds its own clone — `dump_hits`
        // symbolises watchpoint PCs out of it, and the lens panels name addresses from it — so without
        // this it goes on naming addresses out of a listing the engine no longer has, while
        // `emulator/lookup_symbol` answers from the new one. One machine, two answers: the D7 drift,
        // arrived at over the bus instead of over a rebuild.
        //
        // Re-derived from the engine rather than re-read from disk, deliberately: the engine has already
        // parsed and bound the listing the client named, and re-reading the path would be a second parse
        // that can disagree with it. **The armed symbol watches are NOT re-armed** — `SymbolWatch::arm`
        // re-seeds its baselines from live RAM, so doing it here would silently restart every watch's
        // measurement on a gesture that changed no memory; the F5 path re-arms because it really did
        // replace the machine.
        *r.symbols = bus.symbols().cloned();
        crate::notify(
            r.ov,
            ACCENT,
            match r.symbols.as_ref() {
                Some(t) => format!(
                    "aether: symbol listing replaced over the bus — {} symbols",
                    t.len()
                ),
                None => "aether: the symbol listing was dropped over the bus".to_string(),
            },
        );
    }
    // Conflict 1's inbound half: `emulator/pause` / `emulator/resume` are the client's way of stopping and
    // starting *this* loop, and they only mean anything if the loop follows them.
    let bus_paused = bus.is_paused();
    if bus_paused != *r.paused {
        *r.paused = bus_paused;
        crate::notify(
            r.ov,
            ACCENT,
            if bus_paused {
                "aether: paused by a client"
            } else {
                "aether: resumed by a client"
            },
        );
    }

    // --- The display layer mask, read back AFTER the drain and applied to the picture. ---
    //
    // Read from the bus rather than kept here, because there is exactly one mask and it lives on the
    // engine: a client's `emulator/set_layer_enabled` and this window's palette toggles move the same
    // field, so a mask set over the socket reaches the glass on this very iteration and the two can never
    // describe different pictures. Read *after* the drain for that reason — before it, a client's change
    // would show up a frame late.
    //
    // It rides in this function rather than staying in the loop because it is the same thing as
    // everything above it: a client changed the machine and this window has to answer for it. `is_all()`
    // is the whole gate — with nothing hidden, not one line of this runs and the presented picture is
    // byte-for-byte the captured frame the loop has always shown.
    let layers = bus.layers();
    if !layers.is_all() {
        *r.width = crate::blit_masked(sys.vdp(), layers, r.buf);
    }

    Drained {
        resync_audio,
        layers,
    }
}

// -------------------------------------------------------------------------------------------------
// ⚑ The reproduction: the defect that was found by hand, in a test that goes red without the fix
// -------------------------------------------------------------------------------------------------

#[cfg(all(test, feature = "aether"))]
mod tests {
    use super::*;
    use crate::bus::MachineInfo;
    use crate::HEIGHT;
    use oracle_core::scanline_capture::Retain;
    use serde_json::{json, Value};
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    /// Everything [`drain`] writes, owned, so a test can hold one of these across iterations and read the
    /// state back exactly as `fn main` would.
    struct Win {
        ov: Overlay,
        draws: u64,
        cap: ScanlineCapture,
        buf: Vec<u32>,
        width: usize,
        rom_fp: u64,
        symbols: Option<SymbolTable>,
        paused: bool,
    }

    impl Win {
        fn new(rom_fp: u64, symbols: Option<SymbolTable>, paused: bool) -> Self {
            Self {
                ov: Overlay::new(),
                draws: 0,
                cap: ScanlineCapture::new(Retain::LastFrame),
                buf: Vec::new(),
                width: 0,
                rom_fp,
                symbols,
                paused,
            }
        }

        /// One iteration's reaction, in exactly the shape `fn main` calls it.
        fn drain(&mut self, sys: &mut System, bus: &mut Bus) -> Drained {
            super::drain(
                sys,
                bus,
                Reaction {
                    ov: &mut self.ov,
                    draws: &mut self.draws,
                    cap: &mut self.cap,
                    buf: &mut self.buf,
                    width: &mut self.width,
                    rom_fp: &mut self.rom_fp,
                    symbols: &mut self.symbols,
                    paused: &mut self.paused,
                },
            )
        }

        /// The toasts this window has been shown, oldest first.
        fn toasts(&self) -> Vec<String> {
            self.ov.toasts().map(|t| t.text.clone()).collect()
        }

        fn said(&self, needle: &str) -> bool {
            self.toasts().iter().any(|t| t.contains(needle))
        }
    }

    /// A minimal NDJSON client over the socket the bus really binds.
    ///
    /// **Not `bus.rs`'s `Peer`, and the difference is the whole point.** That one owns the pump: its
    /// `call` runs `bus.pump` itself while waiting for a reply. Here the pump must be [`drain`] — the
    /// thing under test — so this Peer only sends and polls, and every test below turns the crank with
    /// `Win::drain`. Reusing the other harness would mean the reaction never ran while the reply was in
    /// flight, which is exactly the arrangement this parcel exists to make impossible.
    struct Peer {
        w: UnixStream,
        r: BufReader<UnixStream>,
        id: u64,
    }

    impl Peer {
        fn connect(path: &Path) -> Self {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match UnixStream::connect(path) {
                    Ok(s) => {
                        s.set_read_timeout(Some(Duration::from_millis(10)))
                            .expect("a read timeout, so the drain gets a turn");
                        let r = BufReader::new(s.try_clone().expect("clone the stream"));
                        return Self { w: s, r, id: 1 };
                    }
                    Err(e) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(5));
                        let _ = e;
                    }
                    Err(e) => panic!("connect: {e}"),
                }
            }
        }

        fn send(&mut self, line: &Value) {
            writeln!(self.w, "{line}").expect("write");
            self.w.flush().expect("flush");
        }

        /// Send a request; the returned id is what [`Peer::poll`] matches on.
        fn request(&mut self, method: &str, params: Value) -> u64 {
            let id = self.id;
            self.id += 1;
            self.send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
            id
        }

        /// One non-blocking look for the reply to `id`. `None` means "not yet" — the caller drains again.
        fn poll(&mut self, id: u64) -> Option<Value> {
            let mut line = String::new();
            match self.r.read_line(&mut line) {
                Ok(0) => panic!("the server closed the connection"),
                Ok(_) => {
                    let v: Value = serde_json::from_str(&line).expect("NDJSON");
                    (v["id"] == json!(id)).then_some(v)
                }
                Err(_) => None, // read timeout: give the drain another turn
            }
        }
    }

    /// Send `method` and turn the crank **through [`drain`]** until its reply lands. Returns the reply's
    /// `result`, and every effect of the call has by then been applied to `win` by the seam.
    fn call(
        peer: &mut Peer,
        win: &mut Win,
        sys: &mut System,
        bus: &mut Bus,
        method: &str,
        params: Value,
    ) -> Value {
        let id = peer.request(method, params);
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            win.drain(sys, bus);
            if let Some(v) = peer.poll(id) {
                assert!(v.get("error").is_none(), "{method}: {}", v["error"]);
                return v["result"].clone();
            }
            assert!(Instant::now() < deadline, "{method}: no reply");
        }
    }

    /// A socket path short enough for `sun_path`, unique per process and per thread.
    fn sock(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "or-dr-{tag}-{}-{:?}.sock",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&p);
        p
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oracle-drain-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A minimal ROM image carrying the `deb2` appendix at `end` — the offset a listing's `EndOfRom`
    /// names, and the only thing `SymbolTable::validate_against_rom` can probe.
    fn rom_with_appendix(end: usize) -> Vec<u8> {
        let mut rom = vec![0u8; end + 0x4000];
        rom[end] = 0xDE;
        rom[end + 1] = 0xB2;
        rom
    }

    /// A minimal listing declaring `EndOfRom` at `end` plus one ordinary symbol. Same fixture shape as
    /// `symbol_file.rs`'s, so the two agree about what a listing looks like.
    fn listing_for(end: u32) -> String {
        format!("  Symbol Table (* = unused):\n\n Main : 300 C |\n EndOfRom : {end:X} C |\n\n   2 symbols\n")
    }

    /// A connected, initialised client on a bus serving `path`, with the handshake already turned through
    /// the seam.
    fn connected(path: &Path, win: &mut Win, sys: &mut System, bus: &mut Bus) -> Peer {
        let mut peer = Peer::connect(path);
        call(
            &mut peer,
            win,
            sys,
            bus,
            "initialize",
            json!({
                "clientId": "frontend-drain-test",
                "clientName": "drain seam",
                "clientVersion": "0",
                "protocolVersion": 1,
                "clientCapabilities": {"events": false},
            }),
        );
        peer.send(&json!({"jsonrpc":"2.0","method":"initialized"}));
        peer
    }

    // ---------------------------------------------------------------------------------------------
    // ★ THE REPRODUCTION
    // ---------------------------------------------------------------------------------------------

    /// **The 2026-09-03 defect, reproduced.** A client reloads a cartridge the loaded listing does not
    /// bind to; the engine drops the listing on the D7 check; and the window's own clone of that listing
    /// must go with it.
    ///
    /// Before the fix, the loop reacted to `rom_changed` alone — re-keying the save-state fingerprint and
    /// posting a toast — and never touched `symbols`. This window then named addresses out of a listing
    /// the engine had discarded while `emulator/lookup_symbol` answered out of nothing. Deleting the
    /// `symbols_changed` arm of [`drain`] puts that behaviour back, and this test goes red on it.
    ///
    /// **What could make this go green for the wrong reason, and what rules each out:**
    ///
    /// * *The window never had a listing.* Asserted `is_some()` with its symbol resolved **before** the
    ///   reload, so the `None` below is a drop and not an absence.
    /// * *The reload did not happen.* The reply's `reloaded`/`symbolsDropped` are read back, so a refused
    ///   or no-op reload fails here rather than passing as "nothing changed".
    /// * *The reload happened but the listing still bound.* `symbolsDropped: true` is asserted, so a
    ///   fixture whose two images accidentally agree on the appendix is a failure, not a pass.
    /// * *The seam re-derives symbols every iteration anyway.* It does not — nothing outside the
    ///   `symbols_changed` arm writes `symbols` — and the quiet-drain test below pins that a drain with
    ///   nothing queued writes nothing at all.
    #[test]
    fn a_client_reload_that_drops_the_listing_drops_the_windows_clone_too() {
        let dir = scratch("reload-drop");
        // Two cartridges. The listing's `EndOfRom` finds the appendix in the first and finds zeros in the
        // second, which is exactly `RomBinding::Mismatch(NoAppendixMagic)` — the D7 drop.
        let bound = dir.join("bound.bin");
        let stranger = dir.join("stranger.bin");
        std::fs::write(&bound, rom_with_appendix(0x8000)).unwrap();
        std::fs::write(&stranger, vec![0u8; 0x8000 + 0x4000]).unwrap();

        let table = SymbolTable::parse(&listing_for(0x8000)).expect("the fixture listing parses");
        assert_eq!(
            table.address_of("Main"),
            Some(0x300),
            "the fixture listing must resolve, or the drop below proves nothing"
        );

        let mut sys = System::new(0x5EED);
        sys.load_rom(std::fs::read(&bound).unwrap());
        sys.reset();
        let rom_fp_before = save_state::rom_fingerprint(sys.rom());

        let path = sock("reload-drop");
        let mut bus = Bus::start(
            Some(Some(path.clone())),
            MachineInfo {
                rom_path: Some(bound.display().to_string()),
                symbols: Some(table.clone()),
                symbols_path: Some(dir.join("bound.lst").display().to_string()),
            },
        );

        // The window is paused, which is what `emulator/reload_rom` requires — and, deliberately, means
        // this window runs **no frames of its own** for the whole test. Anything that moves below was
        // moved by the seam.
        let mut win = Win::new(rom_fp_before, Some(table), true);
        let mut peer = connected(&path, &mut win, &mut sys, &mut bus);

        // The control, and it comes first: the window holds the listing and can resolve out of it.
        assert!(
            win.symbols.is_some(),
            "the fixture must start with a listing in the window's own cache"
        );
        assert_eq!(
            win.symbols.as_ref().unwrap().address_of("Main"),
            Some(0x300),
            "…and it must be the listing under test"
        );
        assert_eq!(win.draws, 0, "this window has run no frames of its own");

        let out = call(
            &mut peer,
            &mut win,
            &mut sys,
            &mut bus,
            "emulator/reload_rom",
            json!({"path": stranger.display().to_string()}),
        );
        assert_eq!(
            out["reloaded"],
            json!(true),
            "the reload must have happened"
        );
        assert_eq!(
            out["symbolsDropped"],
            json!(true),
            "the engine must have dropped the listing on the D7 check, or there is nothing for the \
             window to follow: {out}"
        );

        // ★ The assertion the hand-found defect fails.
        assert!(
            win.symbols.is_none(),
            "the window is still holding a listing the engine has discarded — this is the D7 drift the \
             `symbols_changed` arm exists to prevent, and it is what shipped until 2026-09-03"
        );
        assert!(
            win.said("symbol listing was dropped"),
            "a drop the window never mentioned is a drop nobody can see: {:?}",
            win.toasts()
        );

        // The half that always worked, re-established here so a future edit cannot trade one for the
        // other: the cartridge change still re-keys the save-state slots.
        assert_ne!(
            win.rom_fp, rom_fp_before,
            "the save-state key must follow the cartridge"
        );
        assert_eq!(
            win.rom_fp,
            save_state::rom_fingerprint(sys.rom()),
            "…and it must be the key of the image that is actually loaded"
        );
        assert!(win.said("cartridge was replaced"), "{:?}", win.toasts());

        drop(bus);
        let _ = std::fs::remove_file(&path);
    }

    /// The other direction of the same field: a client **installs** a listing over the bus and the
    /// window's clone picks it up. `reload_rom`'s drop and `load_symbols`'s install are the two routes
    /// into `symbols_changed`, and a seam that handled only the drop would pass the test above.
    ///
    /// **Alternative green ruled out:** the window starts with `symbols: None` and is asserted `None`
    /// before the call, so the `Some` at the end cannot be the fixture's own table surviving.
    #[test]
    fn a_client_loading_a_listing_installs_it_in_the_windows_clone() {
        let dir = scratch("load-symbols");
        let rom = dir.join("game.bin");
        let lst = dir.join("game.lst");
        std::fs::write(&rom, rom_with_appendix(0x8000)).unwrap();
        std::fs::write(&lst, listing_for(0x8000)).unwrap();

        let mut sys = System::new(0x5EED);
        sys.load_rom(std::fs::read(&rom).unwrap());
        sys.reset();

        let path = sock("load-symbols");
        let mut bus = Bus::start(
            Some(Some(path.clone())),
            MachineInfo {
                rom_path: Some(rom.display().to_string()),
                symbols: None,
                symbols_path: None,
            },
        );
        let mut win = Win::new(save_state::rom_fingerprint(sys.rom()), None, true);
        let mut peer = connected(&path, &mut win, &mut sys, &mut bus);

        assert!(
            win.symbols.is_none(),
            "the control: this window starts with no listing at all"
        );

        let out = call(
            &mut peer,
            &mut win,
            &mut sys,
            &mut bus,
            "emulator/load_symbols",
            json!({"path": lst.display().to_string()}),
        );
        assert_eq!(
            out["symbolCount"],
            json!(2),
            "the engine must have parsed the fixture listing: {out}"
        );

        let t = win
            .symbols
            .as_ref()
            .expect("the window must have picked up the listing the client installed");
        assert_eq!(t.len(), 2, "…the whole of it, not a placeholder");
        assert_eq!(
            t.address_of("Main"),
            Some(0x300),
            "…and it must resolve the symbol the listing declares"
        );
        assert!(
            win.said("symbol listing replaced over the bus — 2 symbols"),
            "{:?}",
            win.toasts()
        );
        // The cartridge did not move, so the save-state key must not have. A `symbols_changed` arm that
        // re-keyed slots would invalidate every save the human had written, for a gesture that replaced
        // no cartridge.
        assert_eq!(
            win.rom_fp,
            save_state::rom_fingerprint(sys.rom()),
            "loading a listing replaces no cartridge and must not re-key a single slot"
        );
        assert!(
            !win.said("cartridge was replaced"),
            "a listing change must not be reported as a cartridge change: {:?}",
            win.toasts()
        );

        drop(bus);
        let _ = std::fs::remove_file(&path);
    }

    /// **The timeline half.** A client runs frames on a machine this window is not advancing, and the
    /// three repairs have to happen: the frames are counted, the capture's stale lines are dropped, and
    /// audio is told the timeline moved.
    ///
    /// **The vacuity this is written against:** a window that runs its own frames would move `draws`
    /// whatever the seam did. So it starts **paused and runs nothing of its own** — `draws == 0` is
    /// asserted before the call — which makes every frame counted below one the client ran.
    ///
    /// **Alternative green for the capture:** an empty capture is empty either way. The capture is
    /// deliberately filled with a real frame's lines first and asserted non-empty, so the clear is a
    /// clear rather than a coincidence.
    #[test]
    fn a_client_driven_run_is_counted_cleared_and_resynchronised() {
        let mut sys = System::new(0x5EED);
        sys.load_rom(oracle_core::testrom::build());
        sys.reset();

        let path = sock("timeline");
        let mut bus = Bus::start(Some(Some(path.clone())), MachineInfo::default());
        let mut win = Win::new(save_state::rom_fingerprint(sys.rom()), None, true);
        let mut peer = connected(&path, &mut win, &mut sys, &mut bus);

        // Fill the loop's capture with lines from *this* timeline, so the clear below is observable.
        sys.run_frames_with_sink(1, &mut win.cap);
        assert!(
            !win.cap.lines().is_empty(),
            "the control: the capture must be holding lines, or `clear` proves nothing"
        );
        assert_eq!(
            win.draws, 0,
            "the window is paused and runs no frames of its own — every frame counted below is the \
             client's"
        );

        let id = peer.request("emulator/run_frames", json!({"frames": 3}));
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut resynced = false;
        let out = loop {
            resynced |= win.drain(&mut sys, &mut bus).resync_audio;
            if let Some(v) = peer.poll(id) {
                assert!(v.get("error").is_none(), "run_frames: {}", v["error"]);
                break v["result"].clone();
            }
            assert!(Instant::now() < deadline, "run_frames: no reply");
        };
        assert_eq!(out["frames"], json!(3), "the client's run: {out}");

        assert_eq!(
            win.draws, 3,
            "the frames a client ran are frames this window drew, and the counter is what the status \
             line shows a human"
        );
        assert!(
            win.cap.lines().is_empty(),
            "the capture is holding lines from before the client's run"
        );
        assert!(
            resynced,
            "audio was never told the timeline moved — this is the one effect the seam reports instead \
             of performing, so a lost flag is a silent audible burp of the past"
        );

        drop(bus);
        let _ = std::fs::remove_file(&path);
    }

    /// **The picture half.** The bus's own run drew a frame into the bus's capture, not the loop's, so
    /// after `screen_changed` the window has to pull that frame in or present one from before the run.
    ///
    /// **Alternative green ruled out:** `width`/`buf` start at `0`/empty and are asserted so, and the
    /// width is checked against the geometry the test ROM actually programs rather than "non-zero".
    #[test]
    fn a_client_driven_run_reaches_the_windows_framebuffer() {
        let mut sys = System::new(0x5EED);
        sys.load_rom(oracle_core::testrom::build());
        sys.reset();

        let path = sock("screen");
        let mut bus = Bus::start(Some(Some(path.clone())), MachineInfo::default());
        let mut win = Win::new(save_state::rom_fingerprint(sys.rom()), None, true);
        let mut peer = connected(&path, &mut win, &mut sys, &mut bus);

        assert_eq!(
            win.width, 0,
            "the control: no picture has been presented yet"
        );
        assert!(win.buf.is_empty(), "the control: the framebuffer is empty");

        call(
            &mut peer,
            &mut win,
            &mut sys,
            &mut bus,
            "emulator/run_frames",
            json!({"frames": 1}),
        );

        // The width the machine itself is programmed for — derived, not a literal guess.
        let expected = sys.vdp().render_line(0).len();
        assert_eq!(
            win.width, expected,
            "the window must present the frame the client's run drew, at the geometry it drew it in"
        );
        assert_eq!(
            win.buf.len(),
            expected * HEIGHT,
            "a whole frame, not a partial one"
        );

        drop(bus);
        let _ = std::fs::remove_file(&path);
    }

    /// **The pause mirror, inbound.** `emulator/pause` and `emulator/resume` are a client's way of
    /// stopping and starting *this* loop; the seam is where the loop follows them.
    ///
    /// **Alternative green ruled out:** the window starts **running**, so the paused state below is one
    /// the client had to create. Written the obvious way — start paused and assert paused — the test
    /// would pass against a seam that never read the bus back at all.
    #[test]
    fn a_client_can_pause_and_resume_this_window() {
        let mut sys = System::new(0x5EED);
        sys.load_rom(oracle_core::testrom::build());
        sys.reset();

        let path = sock("pause");
        let mut bus = Bus::start(Some(Some(path.clone())), MachineInfo::default());
        let mut win = Win::new(save_state::rom_fingerprint(sys.rom()), None, false);
        let mut peer = connected(&path, &mut win, &mut sys, &mut bus);
        assert!(!win.paused, "the control: this window starts running");

        call(
            &mut peer,
            &mut win,
            &mut sys,
            &mut bus,
            "emulator/pause",
            json!({}),
        );
        assert!(
            win.paused,
            "the client paused the machine and the window kept running — the two would then disagree \
             about whether the game is moving"
        );
        assert!(win.said("paused by a client"), "{:?}", win.toasts());

        call(
            &mut peer,
            &mut win,
            &mut sys,
            &mut bus,
            "emulator/resume",
            json!({}),
        );
        assert!(!win.paused, "the mirror is not a one-way latch");
        assert!(win.said("resumed by a client"), "{:?}", win.toasts());

        drop(bus);
        let _ = std::fs::remove_file(&path);
    }

    /// **The layer mask reaches the glass on the iteration it was set.** A client hides a plane; the very
    /// next drain must repaint `buf` through the mask rather than leaving last frame's picture up.
    ///
    /// **Alternative green ruled out:** the two pictures are compared against each other, so a
    /// `blit_masked` that ignored its mask, or a mask that never reached the seam, leaves them equal and
    /// fails. The unmasked picture is also asserted non-uniform first — a blank screen would compare
    /// equal to anything.
    #[test]
    fn a_client_hiding_a_plane_repaints_the_windows_picture() {
        use oracle_core::render::Layer;

        let mut sys = System::new(0x5EED);
        sys.load_rom(oracle_core::testrom::build());
        sys.reset();
        // A scene where hiding plane A actually changes a dot — an opaque plane-A cell over an opaque
        // plane-B one, in visibly different colours. Programmed straight onto the machine's VDP (and no
        // frame run afterwards, so nothing overwrites it) because the *default* picture of the test ROM
        // is uniform black, and against a uniform picture "the mask repainted it" and "the mask was
        // ignored" produce byte-identical buffers. This is the same fixture
        // `the_masked_picture_is_the_cores_masked_render_and_differs_from_the_unmasked_one` uses, on a
        // `System` instead of a bare `Vdp`.
        {
            let v = sys.vdp_mut();
            let mut reg =
                |r: u8, val: u8| v.control_write(0x8000 | (u16::from(r) << 8) | u16::from(val), 0);
            reg(0x01, 0x74); // display on, mode 5 — before $0C, the mode-4 register mask drops it
            reg(0x0C, 0x81); // H40
            reg(0x02, 0x30); // plane A nametable @ $C000
            reg(0x04, 0x07); // plane B nametable @ $E000
            reg(0x05, 0x58); // SAT @ $B000, empty
            reg(0x07, 0x25); // a non-black backdrop, so "hidden" is not confusable with "black"
            reg(0x0F, 0x02);
            reg(0x10, 0x00);
            let mut write = |code: u8, addr: u16, words: &[u16]| {
                v.control_write((u16::from(code) & 0x03) << 14 | (addr & 0x3FFF), 0);
                v.control_write((u16::from(code) >> 2) << 4 | (addr >> 14), 0);
                for w in words {
                    v.data_write(*w);
                }
            };
            write(0x01, 0x0AA0, &[0x3333; 16]); // pattern $055, solid nibble 3
            write(0x01, 0x0CC0, &[0x5555; 16]); // pattern $066, solid nibble 5
            write(0x01, 0xC000, &[(1 << 13) | 0x055]); // plane A cell (0,0)
            write(0x01, 0xE000, &[(2 << 13) | 0x066]); // plane B cell (0,0), underneath it
            write(0x03, 0x13 * 2, &[0x000E]); // plane A dot: red
            write(0x03, 0x25 * 2, &[0x0E00]); // plane B dot: blue
        }

        // What the window should be showing with nothing hidden, and what it should show with plane A
        // hidden — both from the core, so this test never re-derives the picture itself.
        let mut unmasked = Vec::new();
        let w = crate::blit_masked(sys.vdp(), LayerMask::ALL, &mut unmasked);
        let mut expected_masked = Vec::new();
        let mut hidden = LayerMask::ALL;
        assert!(hidden.set(Layer::PlaneA, false));
        crate::blit_masked(sys.vdp(), hidden, &mut expected_masked);
        // The fixture control. Without a dot that actually moves under the mask, every assertion below
        // would pass against a seam that never read `bus.layers()` at all.
        assert_ne!(
            unmasked, expected_masked,
            "the fixture's picture does not change when plane A is hidden — nothing below could measure \
             anything"
        );

        let path = sock("layers");
        let mut bus = Bus::start(Some(Some(path.clone())), MachineInfo::default());
        let mut win = Win::new(save_state::rom_fingerprint(sys.rom()), None, true);
        let mut peer = connected(&path, &mut win, &mut sys, &mut bus);

        // A drain with the mask still full must leave the picture alone — that is the `is_all()` gate,
        // and it is what makes the change below attributable to the mask.
        let d = win.drain(&mut sys, &mut bus);
        assert!(
            win.buf.is_empty(),
            "with nothing hidden the seam must not touch the framebuffer at all"
        );
        assert!(d.layers.is_all(), "nothing is hidden yet");

        let id = peer.request(
            "emulator/set_layer_enabled",
            json!({"layer": "planeA", "enabled": false}),
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut d_masked = win.drain(&mut sys, &mut bus);
        loop {
            if let Some(v) = peer.poll(id) {
                assert!(
                    v.get("error").is_none(),
                    "set_layer_enabled: {}",
                    v["error"]
                );
                break;
            }
            assert!(Instant::now() < deadline, "set_layer_enabled: no reply");
            d_masked = win.drain(&mut sys, &mut bus);
        }

        assert_eq!(
            win.width, w,
            "the masked picture keeps the machine's geometry"
        );
        assert_eq!(
            win.buf, expected_masked,
            "the window is not showing the core's masked render — either the mask never reached the \
             seam or `blit_masked` ignored it"
        );
        assert_ne!(
            win.buf, unmasked,
            "the window is still showing plane A after a client hid it"
        );
        // The badge the status line draws must describe the mask that painted this picture, which is why
        // the seam hands it back rather than letting the caller re-read `bus.layers()` after the fact.
        assert!(
            !d_masked.layers.is_all(),
            "the drain must report the mask it painted with"
        );
        // …and it goes back. Un-hiding must restore exactly the picture that was there before, which
        // rules out "the buffer changed for some other reason".
        bus.set_layer(Layer::PlaneA, true);
        let d = win.drain(&mut sys, &mut bus);
        assert!(d.layers.is_all(), "the plane is visible again");
        assert_eq!(
            win.buf, expected_masked,
            "with nothing hidden the seam must leave the retained picture exactly as it found it — \
             re-blitting on an unmasked iteration would undo the `is_all()` gate the loop relies on"
        );

        drop(bus);
        let _ = std::fs::remove_file(&path);
    }

    /// **A drain with nothing queued must change nothing — loudly.**
    ///
    /// This is the control the tests above lean on. Every one of them argues "the state moved, so the
    /// seam moved it"; that argument is only worth anything if a drain that had nothing to react to
    /// leaves the same state alone.
    #[test]
    fn a_drain_with_nothing_queued_touches_nothing() {
        let dir = scratch("quiet");
        let rom = dir.join("game.bin");
        std::fs::write(&rom, rom_with_appendix(0x8000)).unwrap();
        let table = SymbolTable::parse(&listing_for(0x8000)).expect("parses");

        let mut sys = System::new(0x5EED);
        sys.load_rom(std::fs::read(&rom).unwrap());
        sys.reset();

        let path = sock("quiet");
        let mut bus = Bus::start(
            Some(Some(path.clone())),
            MachineInfo {
                rom_path: Some(rom.display().to_string()),
                symbols: Some(table.clone()),
                symbols_path: None,
            },
        );
        let fp = save_state::rom_fingerprint(sys.rom());
        let mut win = Win::new(fp, Some(table), true);
        // A connected, idle client — the ordinary state of a served window, and the one where an
        // over-eager reaction would do its damage.
        let _peer = connected(&path, &mut win, &mut sys, &mut bus);
        let mclk = sys.scheduler().now();

        for _ in 0..8 {
            let d = win.drain(&mut sys, &mut bus);
            assert!(
                !d.resync_audio,
                "an idle drain must not resynchronise audio"
            );
        }

        assert!(
            win.symbols.is_some(),
            "an idle drain dropped the window's listing — this is the shape a cache that re-derives \
             every iteration would take, and it would make every staleness test above vacuous"
        );
        assert_eq!(
            win.symbols.as_ref().unwrap().address_of("Main"),
            Some(0x300)
        );
        assert_eq!(
            win.rom_fp, fp,
            "no cartridge moved, so no slot may be re-keyed"
        );
        assert_eq!(win.draws, 0, "no frames ran");
        assert!(win.paused, "nobody touched the pause state");
        assert!(win.buf.is_empty(), "no picture was published");
        assert!(
            win.toasts().is_empty(),
            "an idle drain that says something is an idle drain a human has to read: {:?}",
            win.toasts()
        );
        assert_eq!(
            sys.scheduler().now(),
            mclk,
            "an idle drain must not advance the machine"
        );

        drop(bus);
        let _ = std::fs::remove_file(&path);
    }

    /// **★ The anti-vacuity control: the seam writes `symbols` ONLY when the report says to.**
    ///
    /// Without this row the reproduction above is not what it claims to be. Take the seam and move
    /// `*r.symbols = bus.symbols().cloned()` *out* of its `if pumped.symbols_changed` — re-derive every
    /// iteration, unconditionally — and every other test in this file, the reproduction included, stays
    /// green: the window's clone still ends up matching the engine, just for a reason that has nothing to
    /// do with the branch being tested. That is measured, not argued: the mutation was applied on disk and
    /// the other six rows printed `ok`. A staleness test whose subject is refreshed every frame anyway is
    /// the vacuity that has bitten this repo twice, and it is invisible from inside the assertion.
    ///
    /// So this row starts the window's clone and the **engine's deliberately out of step** — the window
    /// holds a listing the bus was never told about — and drains with nothing queued. In step, the two
    /// arrangements are indistinguishable; out of step, an unconditional re-derivation drops the window's
    /// listing on an idle frame and this goes red. The divergence is a probe rather than a state the
    /// running window reaches, which is exactly why it has to be built by hand here.
    ///
    /// **Alternative green ruled out:** `symbols` is asserted `Some` *before* the drains as well as
    /// after, so a fixture that failed to install the listing fails here rather than passing as
    /// "nothing was dropped". And the bus is asserted to hold nothing, so the survival below cannot be
    /// the engine handing the same table back.
    #[test]
    fn an_idle_drain_does_not_re_derive_the_windows_listing() {
        let dir = scratch("no-eager");
        let rom = dir.join("game.bin");
        std::fs::write(&rom, rom_with_appendix(0x8000)).unwrap();
        let table = SymbolTable::parse(&listing_for(0x8000)).expect("parses");

        let mut sys = System::new(0x5EED);
        sys.load_rom(std::fs::read(&rom).unwrap());
        sys.reset();

        let path = sock("no-eager");
        // The bus is told about the cartridge and **not** about the listing …
        let mut bus = Bus::start(
            Some(Some(path.clone())),
            MachineInfo {
                rom_path: Some(rom.display().to_string()),
                symbols: None,
                symbols_path: None,
            },
        );
        // … while the window holds one. Nothing has changed, so nothing may write either side.
        let mut win = Win::new(save_state::rom_fingerprint(sys.rom()), Some(table), true);
        let _peer = connected(&path, &mut win, &mut sys, &mut bus);

        assert!(
            bus.symbols().is_none(),
            "the probe needs the engine to be holding nothing, or an eager re-derivation would be \
             invisible here too"
        );
        assert!(
            win.symbols.is_some(),
            "the control: the window starts with a listing"
        );

        for _ in 0..8 {
            win.drain(&mut sys, &mut bus);
        }

        assert!(
            win.symbols.is_some(),
            "an idle drain re-derived the window's listing from an engine that has none. Nothing in \
             the report asked for that write, and a seam that performs it unconditionally makes the \
             reload reproduction in this file green for the wrong reason"
        );
        assert_eq!(
            win.symbols.as_ref().unwrap().address_of("Main"),
            Some(0x300),
            "…and it is the same listing, not a re-parse"
        );
        assert!(
            win.toasts().is_empty(),
            "an idle drain said something about symbols: {:?}",
            win.toasts()
        );

        drop(bus);
        let _ = std::fs::remove_file(&path);
    }
}
