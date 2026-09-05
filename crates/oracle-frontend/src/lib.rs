//! The parts of the game window that are **not** the game window: the code `oracle-player` needs while the
//! migration onto the toolkit is in flight.
//!
//! ## Why this file exists
//!
//! `docs/OVERSEER.md`'s d-25 ruling rebuilds the debug window on a real UI toolkit, and
//! `docs/2026-09-05-frontend-migration-recon.md` §3.0(b) is the other half: the *game* window migrates onto
//! that same toolkit, feature by feature, while **both windows keep working**. Until this file, the only
//! sharing mechanism between the two binaries was `#[path]` inclusion — a compile-time copy, two crates
//! compiling one file with no shared type identity, and a bug fixed in one window fixed in the other only
//! by luck of the include. Every slice of the migration moves 300-1200 lines; a copy-based migration
//! guarantees the two windows drift apart while it runs.
//!
//! So `oracle-frontend` grows a `lib` target and `oracle-player` depends on it. The bin is unchanged in
//! behaviour: it `use`s these modules out of its own lib instead of declaring them, so `crate::present`
//! still resolves from `overlay.rs` and friends.
//!
//! ## ⚑ What this file must never become, and it would stay green if it did
//!
//! [`pick`]'s `#[cfg(feature = "aether")] mod bus_parity` is the strongest correctness guarantee in this
//! crate: it builds an `oracle_aether::engine::Engine` whose VDP is byte-identical to the one the panel is
//! handed, dispatches `emulator/pixel_attribution`, and asserts **address-level** agreement for every dot
//! of four sprite shapes under all four flip combinations, plus the mask rows and §11.27's colour caveat —
//! reading the mask and `now_mclk` back **off the engine** so the test cannot keep the two sides in step
//! itself. It can only exist in a crate that can see the panel and `oracle_aether::engine::Engine` **at
//! once**.
//!
//! **A lib crate carved out of this one without the `oracle-aether` dependency edge would delete that guard
//! and compile clean.** That is why this is a `lib.rs` *inside `oracle-frontend`* rather than a new
//! `oracle-pick` / `oracle-present` crate: the lib target has this crate's dependency list by construction,
//! `aether` is a default feature, and `cargo test -p oracle-frontend --lib` runs all nine `bus_parity` rows.
//! If a later slice ever splits these modules into their own crate, that crate must depend on
//! `oracle-aether` or the guard goes silently.
//!
//! ## What is here, and what is deliberately not
//!
//! Only modules whose `crate::` edges are **closed** over this set, so the move is a relocation and not a
//! rewrite. That is a smaller set than the recon's §3.0(b) list, and the difference is a finding rather
//! than a shortcut: `config` reaches `crate::gamepad_default_deadzone`, an item defined in `main.rs`;
//! `commands` reaches `crate::lens`, which reaches `crate::overlay`, which reaches `crate::spawn` and
//! `crate::screen_text`; and `drain`/`bus` reach `crate::main`, `crate::blit_masked` and `crate::notify`.
//! None of those can move without first cutting an edge back into the binary's own run loop, which is
//! S3-S6's work and not S0's. See `docs/2026-09-05-frontend-migration-s0-s2.md`.
//!
//! * [`audio`] — the SPSC ring, the `i16→f32` conversion and the cpal output stream. `oracle-player` used
//!   to `#[path]`-include this file byte for byte; it now links it, so the pacing policy tuned by
//!   measurement exists once.
//! * [`font`] — the 5x7 bitmap font and its clipping canvas. Here because [`present`] documents against it.
//! * [`present`] — display geometry: [`present::dest_rect`], the blit, and [`present::window_to_native`],
//!   the *exact inverse* of that blit. The player's Screen tab inverts a click through this.
//! * [`pick`] — click-to-watch: a native dot resolved to armable VRAM/CRAM ranges. Window-independent
//!   already; the three window-coupled calls stayed in the binary's run loop.
//! * [`spawn`] — click-to-place: the spawn mode's model and every sentence it puts on screen.
//!
//! ## The `window` feature
//!
//! `minifb`, `x11-dl` and `raw-window-handle` are now optional and gated behind the default-on `window`
//! feature, which the `oracle-frontend` **binary** requires. Nothing in this lib touches them, so a
//! consumer that wants [`present`] and [`pick`] does not pull a windowing library, an X11 loader and a
//! raw-handle shim in to reach them. That was the standing objection to giving this crate a lib target at
//! all (`oracle-player/src/main.rs`'s `#[path]` note: *"giving `oracle-frontend` a `lib` target drags
//! `minifb`, `x11-dl` and `gilrs` into this crate's graph to reach one file"*), and this is the answer to
//! it. `gilrs` was already optional.

// The player's audio substrate — the SPSC ring, the sink composition and the cpal output stream.
#[cfg(feature = "audio")]
pub mod audio;
// The self-contained 5x7 bitmap font and its clipping canvas. `minifb` presents a raw pixel buffer and has
// no text rendering whatsoever; a toolkit consumer does not need this, but `present` documents against it.
pub mod font;
// The window's desktop identity — the embedded Oracle mark and the WM class. **Both windows' icon, from
// one blob.** The data half (decode, `rgba8`, `net_wm_icon`) is always compiled; `apply`, which takes a
// `minifb::Window`, is behind the `window` feature, so `oracle-player` reaches the mark without linking a
// windowing library. See the module header for why it is here rather than in the binary.
pub mod icon;
// Click-to-watch: resolving a clicked dot to armable VRAM/CRAM ranges, sprites included. **Keep this module
// and the `oracle-aether` edge in one compilation unit** — see the module doc above.
pub mod pick;
// Display geometry — aspect handling, the window-sized presentation blit, and the exact click inverse.
pub mod present;
// Click-to-PLACE: spawn mode's model and every sentence it puts on screen. The click itself belongs to
// whichever window holds it.
pub mod spawn;
