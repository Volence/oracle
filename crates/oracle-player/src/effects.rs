//! **The live raster/parallax switchboard's model** — pick a scene, a raster program or a band table by
//! name and write the cells the engine's own installer writes, so the running game switches to it.
//!
//! `LIVE-EFFECTS`, declared by the owner at empyrean `contract/projects.json` (`declaredAt`
//! 2026-09-06T15:47:55Z, *"go for it"*). It replaces holding START plus a button in the debug build:
//!
//! > *"do you think it'd work to have oracle have a raster/parallax panel where we can select what's on
//! > and at what bands or whatever and change it in the ram live? And then get rid of holding start +
//! > button to switch through them for debug?"*
//!
//! # ⚑ Nothing here holds an egui type, and that is the point rather than the tidiness
//!
//! The rule [`crate::spawn_picker`] opens with, for its reason: this window cannot be opened from an
//! agent seat, so a panel whose correctness lives in its draw calls is a panel nothing can check. The
//! projection is here, testable against values a test chooses; [`crate::ui`] lays it out and decides
//! nothing.
//!
//! # ⚑ WRITING ONE POINTER IS NOT A SELECTION, AND THAT IS THIS MODULE'S CENTRAL CORRECTION
//!
//! The card this parcel was written from says the three selectors are longwords re-read every frame, *"so
//! a write takes effect on the next frame with no engine change"*. **The first half is true and the
//! conclusion does not follow.** Read against aeon's own committed engine at [`ENGINE`], each channel has
//! a companion cell whose stale value either reverts the write or hides it:
//!
//! * **Raster.** `Raster_Program` is an **output of the install, not its input.** `Raster_Install` is one
//!   instruction — `move.l a0, Raster_Pending` (`engine/effects/raster.emp:946`) — and `Raster_VBlank`
//!   is what copies the program into `Raster_Buf_A`, re-points `Raster_Active_Buf` and clears
//!   `Raster_Patch_Tab`. **The HInt walker reads `Raster_Active_Buf`, never `Raster_Program`**
//!   (`raster.emp:1052`). So a panel that wrote `Raster_Program` would name the new program while the
//!   screen kept drawing the old one, and a stale `Raster_Patch_Tab` would re-record the outgoing program
//!   over the buffer every VBlank. That is a display that lies, arrived at by following the note.
//! * **Parallax.** While `Parallax_Transition_Frames` is non-zero, `Parallax_Update` reads
//!   `Parallax_Target_Config` and ignores `Parallax_Current_Config` entirely; when the counter expires it
//!   **promotes the staged target over the write** (`engine/level/parallax.emp:1655-1657`). A lone poke
//!   is ignored for a few frames and then silently reverted.
//! * **Bands.** `BgAnim_SetTable` poisons all eight bytes of `BgAnim_LastStep` alongside the pointer, and
//!   its header says why in the imperative: the state array is per **band index**, not per band identity,
//!   so two tables whose band 0 sits on the same step take the `.skip_band` arm forever and *"the new
//!   table would never paint"*. Poisoning *"makes the switch atomic — there is no way to call this and
//!   get the stale picture."* A lone pointer write is exactly the way.
//!
//! So [`Channel::writes`] is a **write-set**, transcribed cell by cell from the installer each channel
//! has, with the line it came from on every cell. **No engine change is needed for any of it** — every
//! cell is RAM and every value is a literal or a symbol's address, so the card's headline finding
//! survives; what does not survive is the count of cells.
//!
//! ⚑ **And the write-set is atomic because the machine is paused**, which is the second thing
//! pause-write-resume buys and nobody asked for: four cells written to a stopped machine are seen by the
//! next frame together or not at all. On a running machine they would not be.
//!
//! # ⚑ THE ONE PLACE THE ENGINE'S RAM SURFACE IS WRITTEN DOWN
//!
//! [`CHANNELS`] is it. Every symbol name, every transcribed address, every literal and the band record's
//! shape appear once, here, cited to [`NOTE`] or [`ENGINE`] — so a test cannot drift from the panel by
//! pinning a number the panel later changed. Nothing in [`crate::ui`] spells a symbol or an address.
//!
//! # ⚑ EVERY CELL IS ADDRESSED BY NAME. THE TRANSCRIBED ADDRESS IS A WITNESS, NEVER A TARGET
//!
//! [`Channel::noted_addr`] is what the note recorded, **read from `s4.debug.lst`**, and it is used for
//! one thing: comparing against what the loaded listing resolves the same name to.
//!
//! The reason is a defect this workspace has already paid for. `Camera_X` is `$FFFFA576` in the release
//! listing and `$FFFFA604` in the debug one, and unlike a missing symbol **a stale address does not
//! fault — it returns a number**, and a number is what everything downstream believes. The hazard is
//! worse here because this surface *writes*: a poke through a carried address lands in whatever RAM now
//! occupies it, on a machine the person is watching.
//!
//! So every write goes out as `emulator/write_memory {symbol, value, width}` and **the server resolves
//! the destination from its own listing** — the same table `emulator/load_symbols` bound. When the
//! resolved address and the note's disagree the gesture is **refused** rather than resolved in either
//! direction ([`Channel::drift`]): a disagreement means one of the two describes a different build and
//! the panel cannot tell which.
//!
//! # ⚑ `Debug_Lab_Index` IS NEVER WRITTEN, AND IT IS GUARDED TWICE
//!
//! `$FFFFEE0D` is the START chord's **cursor**. Writing it moves the label on screen and changes nothing
//! that runs, which is a display that lies — the worst outcome available to a panel whose job is telling
//! a person what the machine is doing. [`NOTE`] prices it: *"This cost this lane an hour today — the
//! label said one row while the machine ran another."*
//!
//! [`forbidden`] refuses it **by name and by address independently**, because those are two different
//! routes in: a write-set edited to name it, and a cell whose symbol happens to resolve there. Neither
//! guard subsumes the other and both are gated.
//!
//! # ⚑ A PAUSED WRITE CANNOT LAND MID-FRAME, SO THE TORN-FRAME CAVEAT DOES NOT APPLY HERE
//!
//! aeon's first note warned that writing these pointers races the once-per-frame read and yields one torn
//! frame. **It does not apply to this panel and a later reader should not re-derive the worry.** Every
//! gesture is pause, write, resume ([`crate::screen_pick::paused_for`]), and a write to a stopped machine
//! cannot land between a read and a buffer fill because nothing is reading. aeon accepted the correction
//! in the same note that raised it ([`NOTE`] §5): *"Oracle is right that a paused write cannot land
//! mid-frame … the caveat stands only for a write to a running machine, which is the scripted-sweep case
//! and not the panel's."*
//!
//! Pause-write-resume is not merely the owner's preference either. `emulator/write_memory` is
//! **paused-machine only** (`engine.rs`'s `require_paused`, *"refused never clipped"*), so it is what the
//! served contract permits, and the owner accepted the cost in advance: *"a small pause to pause and
//! unpause for the change is fine, it's not like I'll be mid intense game and get mad because I myself
//! change the effect."*
//!
//! # ⚑ NOTHING HERE IS EVER WRITTEN TO A FILE
//!
//! *"if I choose like bands and stuff it's not meant to be permanent, just testing stuff."* Authoring
//! lives in aurora. This module holds no serde derive, no storage key and no path, and
//! [`crate::layout`]'s persistence carries `DockState<Tab>` — the dock's *shape* — and nothing a panel
//! selected. `nothing_this_panel_selects_can_reach_the_saved_layout` in [`crate::layout`] is the gate.

use oracle_frontend::spawn::{Caller, Refusal};

/// **The committed aeon note the RAM surface is transcribed from**, at the ref rather than at a tip.
///
/// A tip moves; a commit does not. The rule is the one this lane pointed back at the hub and had adopted:
/// cite the commit that carries the artifact, never a branch.
pub const NOTE: &str = "aeon c4c5c3d8 docs/2026-09-06-live-effects-ram-surface.md";

/// **The committed engine the write-sets are transcribed from**, at the same ref.
///
/// Separate from [`NOTE`] because they are different sources with different standing: the note is aeon
/// telling this lane what to write, and the engine is what the engine does. Where they disagree the
/// engine wins and the disagreement is reported upward — which is how [`Channel::writes`] came to have
/// more than one cell in it.
pub const ENGINE: &str = "aeon c4c5c3d8 engine/";

/// **The commit that landed the act-independent bands-off target**, cited separately from [`NOTE`]
/// because it postdates it and corrects it: [`NOTE`] §3 says no such symbol exists, and now one does.
pub const EMPTY_TABLE_COMMIT: &str = "aeon 41c845fa";

/// **The chord's cursor, which this panel must never write.** See the module header.
pub const LAB_INDEX_SYMBOL: &str = "Debug_Lab_Index";

/// [`LAB_INDEX_SYMBOL`]'s address as [`NOTE`] records it. Used **only** by [`forbidden`]'s second guard,
/// so a cell whose symbol resolves here is caught even when the name check passes.
pub const LAB_INDEX_ADDR: u32 = 0xFFFF_EE0D;

// -------------------------------------------------------------------------------------------------------
// One cell of a write-set
// -------------------------------------------------------------------------------------------------------

/// What goes into a [`Cell`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Put {
    /// The address of whatever the person picked. Exactly one cell per channel carries this.
    Target,
    /// A literal, transcribed from the installer this write-set copies.
    Lit(u32),
}

/// **One cell of a channel's write-set**, with the line of [`ENGINE`] it was transcribed from.
///
/// `why` is not a comment: it is drawn in the panel beside the cell, because a person looking at four
/// writes where a card promised one is owed the reason for each without leaving the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    /// The symbol naming the cell. **Resolved server-side by the write itself**; never an address here.
    pub symbol: &'static str,
    /// Bytes past `symbol`. `emulator/write_memory`'s `disp`, which is legal only with `symbol` — the
    /// ergonomic half of §11.17, and the reason the second half of an eight-byte state array does not
    /// need a symbol of its own.
    pub disp: u32,
    /// 1, 2 or 4, as the door takes it and as the engine's own instruction writes it.
    pub width: u8,
    /// What to put there.
    pub put: Put,
    /// The line of [`ENGINE`] this cell is transcribed from, and what goes wrong without it.
    pub why: &'static str,
}

// -------------------------------------------------------------------------------------------------------
// The three channels
// -------------------------------------------------------------------------------------------------------

/// **How a channel is turned off**, or why it cannot be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Off {
    /// There is no off state. The string is the reason, drawn where the control would have been.
    No(&'static str),
    /// Run the channel's own write-set with `symbol` as the target.
    At {
        /// The empty program or empty table this channel's off state is.
        symbol: &'static str,
        /// What to say when `symbol` is absent from the listing. **Never a fallback**: the panel refuses
        /// rather than choosing an address of its own.
        blocked: &'static str,
    },
}

/// **One effect channel: what says it is live, what a selection writes, and which shapes have it.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Channel {
    /// The stable key this channel is addressed by in code and in a test. Never drawn.
    pub key: &'static str,
    /// What a person calls it.
    pub title: &'static str,
    /// ⚑ **The cell that says what is LIVE**, which is not always a cell this panel writes.
    ///
    /// The raster channel is what makes the distinction load-bearing rather than pedantic: a selection
    /// writes `Raster_Pending` and `Raster_VBlank` moves it into `Raster_Program` on the next frame, so
    /// this cell is how the panel can say *what you asked for has not taken yet* instead of asserting it
    /// has. See [`live`].
    pub selector: &'static str,
    /// ⚑ The address [`NOTE`] records for [`Channel::selector`], **read from `s4.debug.lst`**. A witness
    /// for [`Channel::drift`] and nothing else. Never written to, never sent.
    pub noted_addr: u32,
    /// **What a selection writes**, in order, transcribed from the engine's own installer.
    pub writes: &'static [Cell],
    /// The installer this write-set copies, named for the panel.
    pub installer: &'static str,
    /// The symbol prefix this channel's candidates are published under, as measured today. **A starting
    /// point offered to a bounded search, not a claim that these are all of them** — the panel lets a
    /// person type another, because a prefix fixed here is this crate asserting a fact about somebody
    /// else's game and it would be wrong the first time that game changed.
    pub prefix: &'static str,
    /// ⚑ **`true` when the channel exists only in a DEBUG build.** Gated on symbol presence and never on
    /// an address: on a release build that address is something else entirely, and a panel keyed on the
    /// number would write into unrelated RAM without a fault to show for it.
    pub debug_only: bool,
    /// How this channel is turned off.
    pub off: Off,
    /// ⚑ **What a ZERO in [`Channel::selector`] means**, transcribed from [`NOTE`] §1 per channel.
    ///
    /// It is a per-channel fact and it is not the same fact: a zero raster program is *no program* and
    /// the engine short-circuits on it, while a zero band pointer is *never valid* because `BgAnim_Init`
    /// seeds it. Rendering both as *"the listing names nothing there"* would report a documented state as
    /// an unreadable one on the channel where it is documented, and an unseeded machine as an ordinary
    /// one on the channel where it is a fault.
    pub zero: &'static str,
    /// One line naming what a selection here changes, for the panel and the standing statement.
    pub subject: &'static str,
}

/// **The parallax scene.** A scene *is* a parallax config; everything else a scene needs is re-derived
/// from the config every frame.
///
/// # ⚑ This write-set is `Parallax_StartTransition`'s INSTANT arm, deliberately, and it is not the whole proc
///
/// The engine's proc picks between two arms on the new config's `pcfg_transition` byte: instant, or a
/// smooth lerp that stages `Parallax_Target_Config` and leaves `Parallax_Current_Config` alone for
/// `PARALLAX_TRANS_DEFAULT` frames. **This panel always takes the instant arm and says so on screen.**
///
/// Two reasons, and the second is the one that matters. A person turning a knob wants the picture to
/// change when they click; and on the smooth arm `Parallax_Current_Config` still holds the *old* scene
/// for several frames, so the panel's own readback would disagree with what the panel just did — the
/// exact class of lying readout this whole surface exists against. The difference from the chord is a
/// real one and is stated rather than hidden.
///
/// # The VDP $0B shadow is deliberately NOT written, and that is measured rather than assumed
///
/// `Parallax_StartTransition` writes the Mode Set 3 shadow, so its absence here looks like an omission.
/// It is not: `Parallax_Update` **re-asserts $0B every frame from the resolved active config**
/// (`parallax.emp:1668-1690`), and its own comment gives the reason — *"Parallax_StartTransition writes
/// the mode only on a section-boundary crossing … without a per-frame re-assert the register goes stale
/// whenever the active config differs from what the last crossing left."* The self-heal is the engine's
/// design, not this panel's luck. It also names itself the **sole writer** of that byte, which is a
/// second reason not to write it from here.
pub const PARALLAX: Channel = Channel {
    key: "parallax",
    title: "scene",
    selector: "Parallax_Current_Config",
    noted_addr: 0xFFFF_88EC,
    installer: "Parallax_StartTransition's instant arm (parallax.emp:1279-1287)",
    writes: &[
        Cell {
            symbol: "Parallax_Current_Config",
            disp: 0,
            width: 4,
            put: Put::Target,
            why: "the settled config. parallax.emp:1284, and the cell Parallax_Update reads on its \
                  .use_current arm (:1663)",
        },
        Cell {
            symbol: "Parallax_Target_Config",
            disp: 0,
            width: 4,
            put: Put::Lit(0),
            why: "clears a staged transition. parallax.emp:1285. WITHOUT IT the write above is \
                  reverted: when the transition counter expires, Parallax_Update promotes the staged \
                  target over it (:1655-1657)",
        },
        Cell {
            symbol: "Parallax_Transition_Frames",
            disp: 0,
            width: 1,
            put: Put::Lit(0),
            why: "stops the counter. parallax.emp:1286. WITHOUT IT Parallax_Update ignores \
                  Current_Config entirely and drives from the staged target instead (:1650)",
        },
        Cell {
            symbol: "Parallax_Snap_Pending",
            disp: 0,
            width: 1,
            put: Put::Lit(1),
            why: "snaps the band scroll to the new config instead of easing to it over \
                  PARALLAX_LERP_SHIFT frames. parallax.emp:1287",
        },
    ],
    prefix: "ParallaxConfig_",
    debug_only: false,
    // There is no off state for a scene, and inventing one would be this panel guessing. `NOTE` documents
    // a zero selector for the raster channel and for neither of the other two, so a zero here is
    // undefined behaviour dressed as a toggle.
    off: Off::No(
        "a scene is always in effect, so there is nothing to turn off. Pick a different one instead.",
    ),
    // `Parallax_Update`'s `.config_resolved` arm reads a zero as inert and takes the `.no_config`
    // early-out (parallax.emp:1665-1666). `NOTE` documents no zero for this selector, so this says what
    // the engine does and does not call it a supported state.
    zero: "no parallax config is active: the engine treats a zero here as inert and takes its no-config \
           early-out. It is not a state this panel offers, so something else put it there",
    subject: "which parallax config the engine reads every frame",
};

/// **The raster program.** A per-line program the raster tier builds from once a frame.
///
/// # ⚑ THE CELL A SELECTION WRITES IS `Raster_Pending`, NOT `Raster_Program`
///
/// The single largest correction this module carries, and it is one longword either way — so the cost of
/// getting it wrong is not effort, it is a picture that lies. `Raster_Install`'s **entire body** is
/// `move.l a0, Raster_Pending` (`raster.emp:946`); `Raster_VBlank` is what then copies the program into
/// `Raster_Buf_A`, points `Raster_Active_Buf` at it, and clears `Raster_Patch_Tab` and
/// `Effects_Offscreen_Entry` (`raster.emp:1023-1039`).
///
/// **The HInt walker reads `Raster_Active_Buf`** (`raster.emp:1052`). Writing `Raster_Program` on its own
/// changes the cell a readout looks at and none of the state the screen is drawn from: the panel would
/// name the new program while the old one kept running. A leftover `Raster_Patch_Tab` makes it worse than
/// inert — `Raster_BuildSchedule` keeps re-recording the *outgoing* patched program over the buffer every
/// VBlank, so *"the static program would never be walked at all"* (`raster.emp:1025-1028`).
///
/// # `Raster_Pending` = 0 is "keep whatever is live", so it is never an off route
///
/// `raster.emp:934-942` is explicit, and the off row is an **empty program**: `Raster_VBlank` recognises
/// one by its first record already being the terminator and tail-calls `HBlank_Uninstall`. Poking
/// `Raster_Program` to the empty program instead would leave the HInt armed — measured by aeon at *"512
/// cycles per frame across TWO HInt entries, forever, to accomplish nothing"*.
pub const RASTER: Channel = Channel {
    key: "raster",
    title: "raster program",
    // What is LIVE. Written by `Raster_VBlank`, never by this panel.
    selector: "Raster_Program",
    noted_addr: 0xFFFF_8BD6,
    installer: "Raster_Install, whose whole body this is (raster.emp:945-948)",
    writes: &[Cell {
        symbol: "Raster_Pending",
        disp: 0,
        width: 4,
        put: Put::Target,
        why: "exactly Raster_Install's whole body (raster.emp:946). Raster_VBlank then copies the \
              program into Raster_Buf_A, re-points Raster_Active_Buf (which is what the HInt walker \
              reads, :1052) and clears Raster_Patch_Tab. Writing Raster_Program instead would name the \
              new program while the screen kept drawing the old one",
    }],
    // The authored programs. Deliberately narrower than `Raster_`, which also matches `Raster_Cursor`,
    // `Raster_Pending` and a dozen other pieces of engine state that are not programs: offering those as
    // selectable would be this panel inviting a person to stage a scratch word as a program.
    prefix: "EditorRaster_",
    debug_only: false,
    off: Off::At {
        // What the chord's own OFF row installs, and what every "raster OFF" preset installs
        // (`preset.emp:434`, whose comment reads "0 here means OFF, never keep").
        symbol: "Raster_Program_None",
        blocked: "the raster tier cannot be turned off from here in this build: `Raster_Program_None` \
                  is not in the loaded listing, and it is the empty program `Raster_VBlank` recognises \
                  in order to uninstall the HInt handler. Staging a zero would mean KEEP whatever is \
                  live, which is the opposite, so nothing is written.",
    },
    // `NOTE` section 1, verbatim in substance: "`Raster_Program` = 0 means no program, and the engine
    // short-circuits on it (`beq` after the read)". A documented state, so it reads as one.
    zero: "no raster program is installed. The engine short-circuits on a zero here, so the per-line \
           tier is off",
    subject: "which per-line raster program the raster tier builds from every frame",
};

/// **The background band table.**
///
/// ⚑ **DEBUG BUILD ONLY.** Measured with a control rather than asserted: `BgAnim_Table_Ptr` occurs 0
/// times in `s4.lst` and once in `s4.debug.lst`, while `Parallax_Current_Config` occurs once in both — so
/// the absence is the listing's and not a broken search. The engine's own reason is in the source:
/// `BgAnim_Update` reads the pointer under `if DEBUG == 1` and does `lea BgAnim_Table` under
/// `if DEBUG == 0` (`bg_anim.emp:250-255`), so in a release build there is no selector and nothing to
/// select.
///
/// # ⚑ The eight poison bytes are half of the switch, not housekeeping
///
/// `BgAnim_SetTable`'s body is the pointer **and** `BgAnim_LastStep` set to the init sentinel
/// (`bg_anim.emp:181-189`), and its header states the requirement rather than the tidiness:
/// `BgAnim_LastStep` is per **band index**, not per band identity, so two tables whose band 0 sits on the
/// same step take the `.skip_band` arm forever and *"the new table would never paint"*. Poisoning
/// *"makes the switch atomic — there is no way to call this and get the stale picture."*
pub const BANDS: Channel = Channel {
    key: "bands",
    title: "band table",
    selector: "BgAnim_Table_Ptr",
    noted_addr: 0xFFFF_E91A,
    installer: "BgAnim_SetTable, whose whole body this is (bg_anim.emp:181-189)",
    writes: &[
        Cell {
            symbol: "BgAnim_Table_Ptr",
            disp: 0,
            width: 4,
            put: Put::Target,
            why: "the table BgAnim_Update walks. bg_anim.emp:183",
        },
        Cell {
            symbol: "BgAnim_LastStep",
            disp: 0,
            width: 4,
            put: Put::Lit(0xFFFF_FFFF),
            why: "the init sentinel, poisoning bands 0 and 1. bg_anim.emp:185. WITHOUT IT a band whose \
                  step matches the outgoing table's takes the .skip_band arm forever and the new table \
                  never paints",
        },
        Cell {
            symbol: "BgAnim_LastStep",
            disp: 4,
            width: 4,
            put: Put::Lit(0xFFFF_FFFF),
            why: "bands 2 and 3, the second half of the same eight-byte state array. bg_anim.emp:186",
        },
    ],
    // Two rows in the shape measured today: the act's own table, and the selector itself, which is drawn
    // and refused rather than hidden (see `Row::offered`). The debug view twins are under `BgAnim_View`,
    // which the prefix box reaches.
    prefix: "BgAnim_Table",
    debug_only: true,
    // ⚑ **THE TARGET'S NAME IS VISIBLE IN RELEASE AND ITS DATA IS NOT**, which is why the destination
    // is gated separately and checked FIRST. The declaration is
    // `pub data BgAnim_Table_Empty: [u16; BGANIM_EMPTY_EMIT] = if DEBUG == 1 { [0] } else { [] }`
    // (`EMPTY_TABLE_COMMIT`), so in a release build the NAME still enters the listing with an address
    // while the array emits nothing: **there is no zero word behind it there.** `BgAnim_Table_Ptr` is
    // genuinely absent from a release listing, so a panel that gated bands-off on the target alone
    // would, on release, write an address with no zero behind it into a cell that is not the band
    // pointer. Two wrongs in one gesture. `turn_off` runs `available` before it resolves this symbol,
    // and that ordering is what makes the target's release visibility harmless rather than dangerous.
    off: Off::At {
        symbol: "BgAnim_Table_Empty",
        // ⚑ The correction this channel exists in the shape it does because of. It looks like bands-off
        // already works: point the selector at the act's own `BgAnim_Table` and the walk returns. It
        // works only because the SHIPPED act's table happens to hold a zero count — `NOTE` §3, "by
        // coincidence of content, not by contract", and `bg_anim.emp`'s own header says the same in the
        // affirmative. An act with live bands holds a real count in that same word, so the obvious
        // implementation turns bands off on the act in front of you and quietly re-installs a table on
        // the next one. Refused until the constant exists.
        blocked: "bands cannot be turned off from this listing: `BgAnim_Table_Empty` is not in it. \
                  That symbol EXISTS in the engine now, so this is almost certainly a listing older \
                  than the build that added it rather than a missing feature. Rebuild the ROM and load \
                  its listing, and this works. There is no substitute and none is attempted: pointing \
                  at the act's own `BgAnim_Table` looks like it works, but only because the shipped act \
                  happens to hold a zero count there, and an act with live bands holds a real one, so \
                  that route turns bands off on one act and quietly re-installs a table on the next. \
                  Zero is not a route either, because `BgAnim_Init` seeds this pointer and a zero in it \
                  is never valid.",
    },
    // `NOTE` section 1: "`BgAnim_Table_Ptr` = 0 is never valid; `BgAnim_Init` seeds it." So a zero is a
    // machine that has not initialised, not a table with no bands, and it is said as the fault it is.
    zero: "NOT A VALID STATE. `BgAnim_Init` seeds this pointer, so a zero means the background \
           animation has not initialised on this machine rather than that its bands are off",
    subject: "which band table the background animation walks every frame",
};

/// **Every channel, in the order the panel offers them.**
pub const CHANNELS: [Channel; 3] = [PARALLAX, RASTER, BANDS];

impl Channel {
    /// The channel with this [`Channel::key`], or `None`.
    pub fn by_key(key: &str) -> Option<Channel> {
        CHANNELS.into_iter().find(|c| c.key == key)
    }

    /// ⚑ **The listing disagrees with [`NOTE`] about where this channel's live cell is.**
    ///
    /// `Some` is a **refusal**, not a caveat, and the asymmetry is deliberate. The obvious reading of a
    /// disagreement is *the note is stale, trust the listing*, and that reading is right about half the
    /// time. The other half is *the listing loaded does not describe the ROM running*, which is the
    /// failure `emulator/load_symbols`' binding check exists for, and which this panel would otherwise
    /// answer by poking a live machine at an address neither side vouches for. The panel cannot tell the
    /// two apart, so it says so and writes nothing.
    ///
    /// Both numbers are named: a refusal that says "they disagree" without saying what they are leaves
    /// the reader nothing to check.
    ///
    /// ⚑ **`resolved` is the listing's 32-bit `rawAddr`**, which is the spelling
    /// [`Channel::noted_addr`] is transcribed in (`$FFFF88EC`, not `$FF88EC`). Comparing the 24-bit door
    /// form would make every channel read as drifted and refuse everything, which is the same class of
    /// mistake as the dead guard in [`forbidden`] with the sign flipped.
    pub fn drift(&self, resolved: u32) -> Option<Refusal> {
        if resolved == self.noted_addr {
            return None;
        }
        Some(Refusal::window(
            "selectorMoved",
            format!(
                "the loaded listing puts `{}` at {resolved:#010X}, and {NOTE} records it at {:#010X}. \
                 Nothing was written. One of the two describes a different build and this panel cannot \
                 tell which: writing to either would poke a running machine at an address neither side \
                 vouches for. The note's addresses were read from `s4.debug.lst`",
                self.selector, self.noted_addr
            ),
            Some(format!(
                "check that the loaded listing is the one this ROM was built with, then re-read {NOTE}"
            )),
        ))
    }
}

/// ⚑ **The guard that keeps [`LAB_INDEX_SYMBOL`] unwritable**, by name and by address independently.
///
/// Two checks rather than one, because they catch two different mistakes and neither subsumes the other:
/// a write-set edited to name the cursor, and a cell whose symbol *resolves* to the cursor's address in
/// some build. A panel that only checked the name would happily poke `$FFFFEE0D` through a symbol called
/// something else; one that only checked the address would miss it the day the cursor moves.
///
/// ⚑ **`raw_addr` is the listing's 32-bit spelling (`rawAddr`), never the 24-bit form the memory doors
/// take**, because [`LAB_INDEX_ADDR`] is transcribed from the note in that spelling. Handing this the
/// door form makes the address branch **dead** — `$FFEE0D` never equals `$FFFFEE0D` — so the guard would
/// pass every real input while its own test went on passing on the constant. That is not hypothetical:
/// it is what this function was doing until
/// `the_address_route_fires_on_a_resolved_symbol_and_not_only_on_the_constant` was written, and the row
/// exists so it cannot come back.
///
/// `None` means the write may proceed. Deliberately not a `bool`: the caller must have a sentence.
pub fn forbidden(symbol: &str, raw_addr: u32) -> Option<Refusal> {
    let how = if symbol == LAB_INDEX_SYMBOL {
        "it is named as the write target"
    } else if raw_addr == LAB_INDEX_ADDR {
        "it resolves to that address"
    } else {
        return None;
    };
    Some(Refusal::window(
        "labIndexIsNotASelector",
        format!(
            "refused: `{LAB_INDEX_SYMBOL}` ({LAB_INDEX_ADDR:#010X}) is the START chord's cursor, not a \
             selector, and {how}. Writing it moves the label on screen and changes nothing that runs, \
             which is a display that lies about what the machine is doing. {NOTE} prices that mistake at \
             an hour of aeon's day: the label said one row while the machine ran another"
        ),
        Some(format!(
            "write one of the cells this panel's write-sets name, never the cursor. The live cells are: \
             {}",
            CHANNELS
                .iter()
                .map(|c| c.selector)
                .collect::<Vec<_>>()
                .join(", ")
        )),
    ))
}

// -------------------------------------------------------------------------------------------------------
// The candidate list
// -------------------------------------------------------------------------------------------------------

/// One candidate as the picker draws it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The symbol, exactly as the loaded listing spells it. What a click sends back, for
    /// [`crate::spawn_picker::Row`]'s reason: a picker that showed a prettified name would be offering a
    /// choice it cannot then make.
    pub name: String,
    /// Whether this is the one **this panel** last pointed the channel at.
    pub selected: bool,
    /// Whether it can be chosen at all.
    pub offered: bool,
    /// Why it cannot be, when [`Row::offered`] is false. The row is **drawn with the reason on it rather
    /// than dropped**, [`crate::spawn_picker::SubtypeRow`]'s rule: a row that vanishes teaches nothing,
    /// and a person who typed a prefix and got fewer rows than the listing holds is owed the difference.
    pub note: Option<String>,
}

/// **The picker's whole surface, as facts.** [`crate::spawn_picker::Listing`]'s shape, deliberately: the
/// facts that are not rows are the same findings, and a second vocabulary for them would be a second
/// thing to keep true.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    /// The rows to draw, already filtered. Empty **iff** [`Listing::absence`] is `Some`.
    pub rows: Vec<Row>,
    /// How many rows are drawn out of how many the search returned, in one small line.
    pub count: String,
    /// The stated line drawn **instead of** rows when there are none (P6: an absent fact is a sentence,
    /// never a blank and never a zero).
    pub absence: Option<String>,
    /// ⚑ **The bounded symbol search was cut short.** See [`truncation`].
    pub truncation: Option<String>,
}

/// ⚑ **The clause that says a list is short without looking short**, or `None` when it is whole.
///
/// `emulator/lookup_symbol`'s prefix search is bounded by the engine's `max_symbol_matches`, **256**
/// (`EngineConfig::default`, `crates/oracle-aether/src/engine.rs`). Past that the reply carries a `total`
/// greater than the items it returned, and a list rendered from those items with nothing said **reads
/// exactly like a complete one** — this panel deciding, on the reader's behalf, that the names it could
/// not fetch do not exist. Loud on unmeasurable, applied to a partial measurement.
///
/// The number is **not** transcribed here: it comes off the reply's own `total`, so a cap change upstream
/// cannot leave this sentence claiming a stale one.
pub fn truncation(shown: usize, total: usize) -> Option<String> {
    (total > shown).then(|| {
        format!(
            "SHOWING {shown} OF {total}. The bus's symbol search is bounded and stopped at {shown}, so \
             this list is NOT all of them. Narrow the prefix to bring the rest into reach."
        )
    })
}

/// **The picker, projected.**
///
/// `names` is what the bounded search returned, `selected` what this panel last pointed the channel at,
/// `total` what the search said there were, and `filter` whatever is in the box. Case-insensitive
/// substring, trimmed, for [`crate::spawn_picker::listing`]'s reason: the point of a filter is finding
/// `ParallaxConfig_Haze_Fast` by typing `haze`.
///
/// ⚑ **A row naming the channel's own live cell is drawn and NOT offered.** `BgAnim_Table` and
/// `BgAnim_Table_Ptr` share a prefix, and pointing a selector at itself makes the engine read the
/// pointer's own bytes as the thing it points to. It is the one target this crate can rule out from the
/// names alone, so it does, out loud.
pub fn listing(
    channel: &Channel,
    names: &[String],
    selected: Option<&str>,
    total: usize,
    filter: &str,
) -> Listing {
    let needle = filter.trim().to_ascii_lowercase();
    let self_ref: Vec<&'static str> = channel
        .writes
        .iter()
        .map(|c| c.symbol)
        .chain(std::iter::once(channel.selector))
        .collect();
    let rows: Vec<Row> = names
        .iter()
        .filter(|n| needle.is_empty() || n.to_ascii_lowercase().contains(&needle))
        .map(|n| {
            let cell = self_ref.iter().find(|s| *s == n);
            Row {
                name: n.clone(),
                selected: selected == Some(n.as_str()),
                offered: cell.is_none(),
                note: cell.map(|s| {
                    format!(
                        "not offered: `{s}` is one of this channel's own cells, not something to point \
                         it at. Selecting it would make the engine read the pointer's own bytes as the \
                         thing it points to."
                    )
                }),
            }
        })
        .collect();

    let absence = if !rows.is_empty() {
        None
    } else if names.is_empty() {
        Some(
            "the search found no names under this prefix. That is an answer rather than a failure: \
             either this build publishes them under a different one, or no listing is loaded. Try a \
             shorter prefix."
                .to_string(),
        )
    } else {
        Some(format!(
            "no name here contains {:?}. The search returned {}; clear the box to see them all.",
            filter.trim(),
            plural(names.len(), "name", "names"),
        ))
    };

    Listing {
        count: format!(
            "{} of {} shown",
            rows.len(),
            plural(names.len(), "name", "names")
        ),
        rows,
        absence,
        // ⚑ Measured against what the SEARCH returned, not against what the FILTER left. A filter hiding
        // rows is the person's own doing and is already reported by `count`; a bounded search hiding
        // rows is the bus's, and is the only one of the two that needs saying.
        truncation: truncation(names.len(), total),
    }
}

/// `1 name` against `2 names`, in one place, because a count that says `1 names` reads as a bug in the
/// number rather than in the sentence.
fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

// -------------------------------------------------------------------------------------------------------
// The bands, read back
// -------------------------------------------------------------------------------------------------------

/// **One band record's size in bytes**, transcribed from [`NOTE`] §2, which pins it with an `ensure` in
/// `engine/level/bg_anim.emp`.
///
/// ⚑ **Taken from the note's stated figure, never from adding up the fields below.** Six `u16` and eight
/// `u32` sum to 44 and that agreement is a *check*, not the derivation: a struct with tail padding, or
/// one field this crate has mis-transcribed, sums to 44 just as readily while the walk strides
/// differently.
pub const BAND_RECORD_BYTES: usize = 44;

/// `BGANIM_MAX_BANDS`, from [`NOTE`] §2. A count larger than this is reported rather than trusted.
pub const MAX_BANDS: usize = 4;

/// The table's header: a `u16` count of the records that follow ([`NOTE`] §2, and `bg_anim.emp:257`'s
/// `move.w (a3)+, d7`).
pub const BAND_COUNT_BYTES: usize = 2;

/// **The three things a band can be driven by**, from [`NOTE`] §2's `driver` row: *"0 = Camera_X,
/// 1 = Camera_Y, 2 = Logic_Tick. Only these three."*
///
/// A fourth value is **named as unknown** rather than folded into one of the three. The point of this
/// readout is telling a person what the machine is doing, and a driver this crate cannot name is a fact
/// about the build rather than a reason to guess.
pub fn driver_name(driver: u16) -> Option<&'static str> {
    match driver {
        0 => Some("Camera_X"),
        1 => Some("Camera_Y"),
        2 => Some("Logic_Tick"),
        _ => None,
    }
}

/// **One band, exactly as [`NOTE`] §2's table lays it out.**
///
/// Field for field, offset for offset, from the committed note. ⚑ **Nothing here was inferred from sample
/// bytes.** A struct layout guessed from one observation is the transpose bug this workspace keeps paying
/// for; the readback was reported BLOCKED for exactly as long as the layout was undocumented, and it is
/// built now because aeon committed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Band {
    /// Offset 0. See [`driver_name`].
    pub driver: u16,
    /// Offset 2. `step = driver_value >> rate_shift`. Larger is slower.
    pub rate_shift: u16,
    /// Offset 4. The pattern's period along the axis in pixels, **minus one** — so 63 means 64 px.
    pub step_mask: u16,
    /// Offset 6. **log2 of the rotation unit in bytes**, so the unit is `1 << col_shift`.
    pub col_shift: u16,
    /// Offset 8. Tiles in the band.
    pub tile_count: u16,
    /// Offset 10. The **VRAM byte address** of the band's first slot.
    pub vram_dest: u16,
    /// Offsets 12 to 44. Pointers to the eight pre-shifted art banks, one pixel apart.
    pub banks: [u32; 8],
}

impl Band {
    /// **One record out of `bytes`**, big-endian as the 68000 stores, or `None` when the slice is short.
    ///
    /// `None` rather than a zero-filled record: a band this crate could not read is not a band of zeroes,
    /// and the difference is the whole of P6.
    pub fn parse(bytes: &[u8]) -> Option<Band> {
        if bytes.len() < BAND_RECORD_BYTES {
            return None;
        }
        let w = |off: usize| u16::from_be_bytes([bytes[off], bytes[off + 1]]);
        let l = |off: usize| {
            u32::from_be_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
        };
        let mut banks = [0u32; 8];
        for (i, b) in banks.iter_mut().enumerate() {
            *b = l(12 + i * 4);
        }
        Some(Band {
            driver: w(0),
            rate_shift: w(2),
            step_mask: w(4),
            col_shift: w(6),
            tile_count: w(8),
            vram_dest: w(10),
            banks,
        })
    }

    /// **The readable line**, with the two derivations [`NOTE`] spells out done here rather than left to
    /// the reader: `step_mask` is a period **minus one**, and `col_shift` is a **log2**. A readout
    /// printing the raw 63 and 7 would hand a person two numbers to convert in their head, and the
    /// off-by-one in the first is exactly the kind that gets converted wrong.
    pub fn line(&self) -> String {
        let driver = match driver_name(self.driver) {
            Some(n) => n.to_string(),
            None => format!(
                "driver {} (UNKNOWN: {NOTE} names only 0, 1 and 2)",
                self.driver
            ),
        };
        format!(
            "{driver} driven, 1 px per {} units, {} px period, rotation unit {} B, {} tiles, first slot \
             at VRAM ${:04X}",
            1u32 << self.rate_shift,
            u32::from(self.step_mask) + 1,
            1u32 << self.col_shift,
            self.tile_count,
            self.vram_dest,
        )
    }
}

/// **The band table as it reads right now**, and everything about it that is not a band.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bands {
    /// The header's own count, carried whole even when it is larger than [`MAX_BANDS`].
    pub count: u16,
    /// The records actually decoded.
    pub bands: Vec<Band>,
    /// The stated line drawn **instead of** rows when there are none (P6). `Some` exactly when
    /// [`Bands::bands`] is empty.
    pub absence: Option<String>,
    /// ⚑ A count the note's own ceiling says cannot be right, or a table this panel could not read whole.
    pub caveat: Option<String>,
}

/// **The table, projected from the bytes at the pointer.**
///
/// `raw` is the header word followed by as much of the records as was read.
pub fn bands(raw: &[u8]) -> Bands {
    if raw.len() < BAND_COUNT_BYTES {
        return Bands {
            count: 0,
            bands: Vec::new(),
            absence: Some(
                "the table's count word could not be read, so nothing here describes the machine."
                    .to_string(),
            ),
            caveat: None,
        };
    }
    let count = u16::from_be_bytes([raw[0], raw[1]]);
    // ⚑ Decoded up to the note's own ceiling and no further, and the excess is **reported** rather than
    // truncated in silence. A count past `BGANIM_MAX_BANDS` is not a bigger table: it is a word that is
    // not a count, and reading records off it would walk RAM belonging to something else. The engine
    // agrees and says so with an assert it can only afford in the debug shape (`bg_anim.emp:258`), which
    // is a second reason for this panel to carry the check rather than lean on the machine's.
    let readable = (count as usize).min(MAX_BANDS);
    let bands: Vec<Band> = (0..readable)
        .map_while(|i| {
            let off = BAND_COUNT_BYTES + i * BAND_RECORD_BYTES;
            raw.get(off..off + BAND_RECORD_BYTES).and_then(Band::parse)
        })
        .collect();

    let caveat = if count as usize > MAX_BANDS {
        Some(format!(
            "THE COUNT WORD READS {count}, AND {NOTE} PUTS BGANIM_MAX_BANDS AT {MAX_BANDS}. That is not \
             a bigger table, it is a word that is not a count, so only the first {MAX_BANDS} were \
             decoded and the rest of this readout should not be trusted."
        ))
    } else if bands.len() < readable {
        Some(format!(
            "THE TABLE SAYS {count} BANDS AND ONLY {} COULD BE READ. The read stopped short, so what is \
             below is part of the table and not the whole of it.",
            bands.len()
        ))
    } else {
        None
    };

    let absence = bands.is_empty().then(|| {
        if count == 0 {
            // The one absence that is a *finding* rather than a failure: this is what bands-off looks
            // like, and a reader who is not told so reads an empty box as a broken panel.
            "The count word is 0, so this table drives no bands and the background animation walks \
             nothing. That is what bands being off looks like from here."
                .to_string()
        } else {
            format!("The table claims {count} bands and none of them could be read.")
        }
    });

    Bands {
        count,
        bands,
        absence,
        caveat,
    }
}

// -------------------------------------------------------------------------------------------------------
// ⚑ The nudge controls, which do not ship, and why they are drawn anyway
// -------------------------------------------------------------------------------------------------------

/// ⚑ **Why there is no live parameter nudging, in the words a person reads off the disabled control.**
///
/// # The choice, and it was between two honest options rather than three
///
/// The owner's card asks for *"numeric nudges"*. They cannot be built: `Parallax_Current_Config` points
/// at **ROM**, so a factor cannot be edited in place, and aeon's hook for it — a RAM scratch config plus
/// a copy-and-repoint entry, the shape `Raster_Buf_A`/`Raster_Buf_B` already use — is **sized and not
/// started** ([`NOTE`] §4). The hub's ruling is explicit: *"nudge controls do not ship until it lands."*
///
/// Shipping a slider that silently does nothing was never on the table; aeon's note bars it in the same
/// sentence the hub ruled on: *"a slider that silently does nothing is worse than an absent one."*
///
/// So the choice was **omit the control** or **draw it disabled with the reason**, and this module draws
/// it. The argument for omitting is that an absent control makes no promise. The argument against is
/// stronger and it is this panel's own thesis applied to itself: the owner asked for nudges by name, and
/// a panel that simply has none reads as *we forgot* or *we could not find it*, which sends him to ask. A
/// disabled control with one readable line answers the question where it is asked, and it disappears by
/// itself the day the hook lands — nobody has to remember to add it back.
///
/// # Two controls when it lands, not four
///
/// [`NOTE`] §2: only `driver` and `rate_shift` are meaningful. `step_mask` and `col_shift` are geometry
/// derived from the art's shape, and moving either without moving the art gives a cadence the art does
/// not have — *"a picture rather than an effect"*. `vram_dest` and `banks` are placement. Written down
/// here so the surface is not designed four-wide and then taken apart.
pub const NUDGE_BLOCKED: &str =
    "Numeric nudging is not available yet. A scene's factors live in ROM, so they cannot be edited in \
     place; aeon is adding a RAM scratch config that this panel will edit instead. Nothing here would \
     have any effect until that lands, so the control is shown off rather than shipped doing nothing. \
     When it arrives it is two numbers, driver and rate shift, and not the whole record.";

// -------------------------------------------------------------------------------------------------------
// ⚑ The standing statement
// -------------------------------------------------------------------------------------------------------

/// **One channel this panel has pointed somewhere**, as the standing statement names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// [`Channel::key`], so a second change on the same channel replaces the first rather than stacking.
    pub key: String,
    /// [`Channel::title`].
    pub title: String,
    /// The name the channel was pointed at.
    pub target: String,
}

/// ⚑ **THE STANDING STATEMENT THAT THIS PANEL IS CHANGING ENGINE STATE**, or `None` when it has changed
/// nothing this session.
///
/// # Standing, not a toast, and that is correctness rather than polish
///
/// The argument is banked twice in this window already and it is the same one both times. The layer mask
/// earned it first — *"the person who set it will forget, and then read a masked picture as the
/// machine's"* — and the spawn badge earned it again, because **a toast expires and the change does
/// not**. This surface is the widest instance yet: a scene swap changes what every frame after it looks
/// like, and the picture it produces is a perfectly ordinary-looking picture of the wrong configuration.
/// A person who has forgotten reads it as the game's own.
///
/// The owner's own framing is what the line has to keep true in his head: *"if I choose like bands and
/// stuff it's not meant to be permanent, just testing stuff."* It is only temporary if something is still
/// saying so an hour later.
///
/// # It names the channel and the target, never merely admits to a mode
///
/// aurora's measured failure, adopted here before it is paid for a second time: a lens that highlighted
/// 1,244 cells perfectly and drew the reaction *"what are the purple boxes"*. *"Something is overridden"*
/// sends a person hunting; *"scene = ParallaxConfig_Haze"* does not.
///
/// # And it says how the override ends, because the answer is not obvious and is not this panel
///
/// Every override here survives only until the camera crosses a section boundary, at which point the
/// engine installs that section's own scene again (`ojz_scroll_test.emp`'s lab header: *"the override
/// survives until the camera crosses a section boundary … A debug override can never leave the act
/// permanently mis-configured"*). A person who does not know that goes looking for a reset button this
/// panel does not have.
pub fn statement(changes: &[Change]) -> Option<String> {
    if changes.is_empty() {
        return None;
    }
    let what = changes
        .iter()
        .map(|c| format!("{} = {}", c.title, c.target))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "THIS PANEL IS OVERRIDING THE RUNNING GAME: {what}. The picture on screen is this window's \
         configuration, not the act's own. Nothing here is saved to any file, and the engine puts the \
         act's own back the moment the camera crosses a section boundary."
    ))
}

/// Fold `change` into `changes`, replacing any earlier change on the same channel.
///
/// Replace rather than append, so the statement always names **what is in effect** rather than a history
/// of what was tried. A list that grew would go on naming a scene two selections ago, which is the
/// staleness the statement exists to prevent, arriving through the statement itself.
pub fn record(changes: &mut Vec<Change>, change: Change) {
    changes.retain(|c| c.key != change.key);
    changes.push(change);
}

// -------------------------------------------------------------------------------------------------------
// The choreography — per-gesture, synchronous, through the served methods
// -------------------------------------------------------------------------------------------------------

/// **The names this build publishes under `prefix`**, and how many the bounded search said there were.
///
/// Per-frame panel bodies read the shared derivation directly (`protocol.md` D15); **a per-gesture
/// command goes through [`Caller::call`]**, which is `Host::call` in this window — synchronous,
/// in-process, no socket. The reason is [`crate::spawn_picker`]'s: the window asks the same handler a
/// socket client asks, and prints what it gets back, refusal included.
///
/// The exact branch is handled for [`oracle_frontend::spawn::archetypes`]'s reason: a symbol named
/// exactly `prefix` answers `exact: true` with no `otherMatches`, and reading that reply's empty page as
/// an empty list would report "nothing found" from a search that found something.
pub fn candidates(c: &mut impl Caller, prefix: &str) -> Result<(Vec<String>, usize), Refusal> {
    let v = c.call(
        "emulator/lookup_symbol",
        serde_json::json!({ "name": prefix }),
    )?;
    if v["exact"] == serde_json::json!(true) {
        let name = v["name"].as_str().unwrap_or_default().to_string();
        return Ok((vec![name], 1));
    }
    let page = &v["otherMatches"];
    let names: Vec<String> = page["items"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| m["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    // `total` off the envelope, never `names.len()`: the two differ exactly when the search was cut, and
    // that difference is the only thing `truncation` has to go on.
    let total = page["total"].as_u64().unwrap_or(names.len() as u64) as usize;
    Ok((names, total))
}

/// **What one symbol resolves to in the listing the machine is running with**, both spellings.
///
/// Resolved **per call and never cached**, the rule §11.26 was amended to impose on `Camera_X` after it
/// was found to move between build shapes. Returns `(addr, raw_addr)`: `addr` is the 24-bit form every
/// memory door takes, `raw_addr` the 32-bit one the listing wrote down.
pub fn resolve(c: &mut impl Caller, name: &str) -> Result<(u32, u32), Refusal> {
    let v = c.call(
        "emulator/lookup_symbol",
        serde_json::json!({ "name": name }),
    )?;
    if v["exact"] != serde_json::json!(true) {
        return Err(Refusal::window(
            "notInListing",
            format!(
                "`{name}` is not a name in the loaded listing. The bus answered a prefix search rather \
                 than an exact hit, so there is nothing here to point at"
            ),
            Some("load the listing this ROM was built with".to_string()),
        ));
    }
    let parse = |k: &str| {
        v[k].as_str()
            .and_then(|s| u32::from_str_radix(s.strip_prefix("0x").unwrap_or(s), 16).ok())
    };
    let Some(addr) = parse("addr") else {
        return Err(Refusal::local(format!(
            "the bus resolved `{name}` but its `addr` was {:?}, which is not an address",
            v["addr"]
        )));
    };
    // ⚑ `rawAddr` is what the assembler stored and is the value a pointer field holds; `addr` is the
    // 24-bit form the memory doors take. They differ for work-RAM symbols (`$FFFF8C64` against
    // `$FF8C64`), and these writes put a POINTER into a longword rather than hand an address to a reader,
    // so the raw spelling is the faithful one. Falls back to `addr` when the reply omits it, which is
    // every branch where the two agree.
    Ok((addr, parse("rawAddr").unwrap_or(addr)))
}

/// **Whether this build has the channel at all**, as a refusal or `Ok`.
///
/// ⚑ **Gated on the symbol, never on an address.** On a release build `BgAnim_Table_Ptr`'s address is
/// something else entirely, so a panel keyed on the number would write into unrelated RAM without a fault
/// to show for it. The absence was measured with a control rather than assumed: the symbol occurs 0 times
/// in `s4.lst` and once in `s4.debug.lst`, while `Parallax_Current_Config` occurs once in both.
pub fn available(c: &mut impl Caller, channel: &Channel) -> Result<u32, Refusal> {
    match resolve(c, channel.selector) {
        // ⚑ The RAW spelling, because both things done with it next, `forbidden` and `Channel::drift`,
        // compare against addresses transcribed from the note in that spelling.
        Ok((_, raw)) => Ok(raw),
        Err(_) if channel.debug_only => Err(Refusal::window(
            "debugOnlyChannel",
            format!(
                "`{}` is not in the loaded listing, and it exists only in a DEBUG build: \
                 `BgAnim_Update` reads it under `if DEBUG == 1` and reaches the act's own table with a \
                 plain `lea` otherwise ({ENGINE}, bg_anim.emp:250-255). There is no selector in this \
                 shape and nothing to select",
                channel.selector
            ),
            Some("run the debug ROM, or load the debug listing, to switch band tables".to_string()),
        )),
        Err(e) => Err(e),
    }
}

/// **What one gesture actually did**, cell by cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wrote {
    /// The name the channel was pointed at.
    pub target: String,
    /// The installer this write-set copies.
    pub installer: String,
    /// One line per cell, in the order they were written.
    pub cells: Vec<String>,
}

impl Wrote {
    /// The headline: what moved and by how many cells. The count is stated because the card promised one
    /// and the engine wanted more, and a reader who sees four writes go past deserves to know that is the
    /// design rather than a bug.
    pub fn line(&self) -> String {
        format!(
            "pointed at {}. {} written, which is {}.",
            self.target,
            plural(self.cells.len(), "cell", "cells"),
            self.installer
        )
    }
}

/// **Point `channel` at `target`, by running its whole write-set.**
///
/// The order the guards run in:
///
/// 1. **[`available`]** — the channel's live cell resolves in this build's listing at all.
/// 2. **[`forbidden`]**, on the live cell's name and resolved address independently.
/// 3. **[`Channel::drift`]**, refusing when the listing and [`NOTE`] disagree about where it lives.
/// 4. **Resolve the target by name**, so the value written is the listing's and not this crate's.
/// 5. **Every cell**, each addressed `{symbol, disp, value, width}` so the server resolves the
///    destination from the same table it would answer a socket client from, and each passed through
///    [`forbidden`] again on its own resolved address.
///
/// The machine must already be paused: `emulator/write_memory` requires it and refuses otherwise with the
/// server's own words. The pause is the caller's ([`crate::screen_pick::paused_for`]) so the
/// capture-and-restore has one implementation in this crate rather than two.
///
/// ⚑ **A cell that fails mid-set leaves the earlier cells written**, and the refusal says which ran. It
/// is not wrapped in a checkpoint: a checkpoint would put the *whole machine* back, undoing frames the
/// person watched go by, and every cell here is a RAM word a subsequent selection overwrites. Saying
/// exactly how far it got is the honest repair, and it is what the readout does.
pub fn point_at(c: &mut impl Caller, channel: &Channel, target: &str) -> Result<Wrote, Refusal> {
    let sel_addr = available(c, channel)?;
    if let Some(r) = forbidden(channel.selector, sel_addr) {
        return Err(r);
    }
    if let Some(r) = channel.drift(sel_addr) {
        return Err(r);
    }
    let (_, value) = resolve(c, target)?;
    run(c, channel, target, value)
}

/// **Turn `channel` off**, by the one route [`Channel::off`] names, or refuse and say why.
///
/// The refusals are the point of this function rather than an edge of it. Two of the three channels
/// cannot simply be switched off, each for its own documented reason, and a control that quietly did
/// nothing — or that pointed at a table which happens to be empty in the act in front of you — is the
/// defect this panel exists to remove rather than a shortcut to shipping it.
pub fn turn_off(c: &mut impl Caller, channel: &Channel) -> Result<Wrote, Refusal> {
    let (symbol, blocked) = match channel.off {
        Off::No(why) => {
            return Err(Refusal::window(
                "noOffState",
                format!("{}: {why}", channel.title),
                None,
            ))
        }
        Off::At { symbol, blocked } => (symbol, blocked),
    };
    let sel_addr = available(c, channel)?;
    if let Some(r) = forbidden(channel.selector, sel_addr) {
        return Err(r);
    }
    if let Some(r) = channel.drift(sel_addr) {
        return Err(r);
    }
    // The empty program or empty table, when the listing has it. **No fallback**: a panel that reached
    // for a second address when the first was missing would be choosing a target in somebody else's RAM.
    match resolve(c, symbol) {
        Ok((_, value)) => run(c, channel, symbol, value),
        Err(_) => Err(Refusal::window(
            "offTargetMissing",
            blocked,
            // ⚑ The remedy names the SYMBOL and the COMMIT that carries it, because the whole point of
            // this refusal's wording is that a person reads "my listing is old" rather than "this is
            // broken". A remedy that said "load a listing" without saying which name is missing leaves
            // them with nothing to check.
            Some(format!(
                "`{symbol}` landed at {EMPTY_TABLE_COMMIT}. Rebuild the ROM, then load the listing that \
                 build produced"
            )),
        )),
    }
}

/// The write-set itself, shared by [`point_at`] and [`turn_off`] so the two cannot write different cells
/// for one channel.
fn run(
    c: &mut impl Caller,
    channel: &Channel,
    target: &str,
    target_value: u32,
) -> Result<Wrote, Refusal> {
    let mut cells = Vec::new();
    for cell in channel.writes {
        let value = match cell.put {
            Put::Target => target_value,
            Put::Lit(v) => v,
        };
        // ⚑ The forbidden guard runs on **every cell**, not only the channel's live one. A write-set is
        // data, and data is what gets edited by somebody who has not read the header.
        let (_, raw) = resolve(c, cell.symbol)?;
        if let Some(r) = forbidden(cell.symbol, raw.wrapping_add(cell.disp)) {
            return Err(r);
        }
        let mut req = serde_json::json!({
            "symbol": cell.symbol,
            "value": value,
            "width": cell.width,
        });
        if cell.disp > 0 {
            // Legal only with `symbol`, which is what this write uses. §11.17's ergonomic half.
            req["disp"] = serde_json::json!(cell.disp);
        }
        match c.call("emulator/write_memory", req) {
            Ok(_) => cells.push(format!(
                "{}{} = {value:#0width$X} ({})",
                cell.symbol,
                if cell.disp > 0 {
                    format!("+{}", cell.disp)
                } else {
                    String::new()
                },
                cell.why,
                width = 2 + 2 * cell.width as usize,
            )),
            // ⚑ Named partial failure. The message says how far the set got, because "the write was
            // refused" over a half-applied installer would send a person looking for a machine in a state
            // it is not in.
            Err(e) => {
                let why = format!(
                    "{}: {} of {} cells were written before `{}` was refused, so this channel is \
                     HALF SET and the engine may act on neither state. The bus said: {}",
                    channel.title,
                    cells.len(),
                    channel.writes.len(),
                    cell.symbol,
                    e.message
                );
                let next = "select again once the machine can be written to".to_string();
                return Err(Refusal::window("writeSetIncomplete", why, Some(next)));
            }
        }
    }
    Ok(Wrote {
        target: target.to_string(),
        installer: channel.installer.to_string(),
        cells,
    })
}

// -------------------------------------------------------------------------------------------------------
// ⚑ What is actually live, which is the answer to the failure this whole panel is written against
// -------------------------------------------------------------------------------------------------------

/// **What [`Channel::selector`] holds right now, and what symbol that is.**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Live {
    /// The longword in the live cell.
    pub value: u32,
    /// The symbol at or before it, when the listing names one.
    pub name: Option<String>,
    /// How far past that symbol the value sits. Non-zero means it is **not** that symbol.
    pub disp: u64,
}

impl Live {
    /// **The line a person reads to check the panel against the machine**, which is the whole point.
    ///
    /// aeon lost an hour to a label that named one row while the machine ran another. This is the
    /// antidote and it is deliberately a **readback rather than an echo**: it says what the cell holds
    /// now, resolved through the listing, and never what the last click asked for.
    ///
    /// A non-zero displacement is called out rather than swallowed, because *"`X` + $12"* and *"`X`"* are
    /// different findings and the first usually means the cell holds something the listing does not name.
    pub fn line(&self, channel: &Channel) -> String {
        let selector = channel.selector;
        // ⚑ A zero is a **stated per-channel finding**, never "the listing does not name it". See
        // [`Channel::zero`]: on the raster channel it is a documented off state, and on the band channel
        // it is a fault. Reporting both as an unnamed address would hide one and alarm about the other.
        if self.value == 0 {
            return format!("{selector} holds 0. {}.", channel.zero);
        }
        match (&self.name, self.disp) {
            (Some(n), 0) => format!("{selector} holds {:#010X}, which is `{n}`.", self.value),
            (Some(n), d) => format!(
                "{selector} holds {:#010X}, which is ${d:X} past `{n}` and so is probably not `{n}` at \
                 all: the listing carries no sizes.",
                self.value
            ),
            (None, _) => format!(
                "{selector} holds {:#010X}, which the loaded listing does not name.",
                self.value
            ),
        }
    }
}

/// **Read `channel`'s live cell and name what it points at.**
///
/// Two calls, both pure reads on a running machine: the longword, then `emulator/lookup_symbol` in the
/// address direction. Nothing here writes and nothing here needs a pause, which is why the panel can draw
/// it beside a selection whose effect has not landed yet.
///
/// ⚑ **This is how the raster channel stays honest.** A selection stages `Raster_Pending`, and
/// `Raster_Program` does not move until the next `Raster_VBlank`, so on a paused machine this line keeps
/// naming the old program until a frame runs. That is true, and a panel that echoed the click instead
/// would be asserting a swap that has not happened.
pub fn live(c: &mut impl Caller, channel: &Channel) -> Result<Live, Refusal> {
    // ⚑ **The drift check guards the READ as well as the write**, which is a coherence property rather
    // than caution: a panel that refuses to write a cell it cannot place, and then prints a confident
    // sentence about that same cell's contents, has answered the harder question and refused the easier
    // one. Whichever of the listing and the note is wrong, this readout is about the wrong four bytes.
    let sel = available(c, channel)?;
    if let Some(r) = channel.drift(sel) {
        return Err(r);
    }
    let value = read_u32_at_symbol(c, channel.selector)?;
    if value == 0 {
        return Ok(Live {
            value,
            name: None,
            disp: 0,
        });
    }
    // A lookup that finds nothing is an answer here rather than a failure: a cell holding an address the
    // listing does not name is a real and reportable state.
    let named = c.call(
        "emulator/lookup_symbol",
        serde_json::json!({ "addr": format!("0x{:06X}", value & 0x00FF_FFFF) }),
    );
    Ok(match named {
        Ok(v) => Live {
            value,
            name: v["name"].as_str().map(str::to_string),
            disp: v["disp"].as_u64().unwrap_or(0),
        },
        Err(_) => Live {
            value,
            name: None,
            disp: 0,
        },
    })
}

/// **The band table as the machine holds it right now**, read through the selector rather than through an
/// address this crate carries.
pub fn read_bands(c: &mut impl Caller) -> Result<Bands, Refusal> {
    let sel = available(c, &BANDS)?;
    if let Some(r) = BANDS.drift(sel) {
        return Err(r);
    }
    let ptr = read_u32_at_symbol(c, BANDS.selector)?;
    if ptr == 0 {
        // `NOTE` §1: "`BgAnim_Table_Ptr` = 0 is never valid; `BgAnim_Init` seeds it." So a zero here is
        // not an empty table, it is a selector that has not been seeded, and reading address 0 as a table
        // would decode the 68000's vector table as bands.
        return Err(Refusal::window(
            "selectorUnseeded",
            format!(
                "`{}` reads 0, which {NOTE} says is never valid: `BgAnim_Init` seeds it, so this is a \
                 machine that has not initialised the background animation rather than a table with no \
                 bands. Nothing was decoded",
                BANDS.selector
            ),
            Some("start an act first, then read again".to_string()),
        ));
    }
    let len = BAND_COUNT_BYTES + MAX_BANDS * BAND_RECORD_BYTES;
    let raw = read_bytes(c, ptr & 0x00FF_FFFF, len)?;
    Ok(bands(&raw))
}

/// One longword out of the location `symbol` names, through the served reader.
fn read_u32_at_symbol(c: &mut impl Caller, symbol: &str) -> Result<u32, Refusal> {
    let r = c.call(
        "emulator/read_memory",
        serde_json::json!({ "symbol": symbol, "len": 4 }),
    )?;
    parse_hex_bytes(&r, 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// `len` bytes from `addr`.
fn read_bytes(c: &mut impl Caller, addr: u32, len: usize) -> Result<Vec<u8>, Refusal> {
    let r = c.call(
        "emulator/read_memory",
        serde_json::json!({ "addr": format!("0x{addr:08X}"), "len": len }),
    )?;
    parse_hex_bytes(&r, len)
}

/// The reply's `bytes` hex string, or a refusal naming what came back instead.
///
/// A short or unparseable reply is **refused, never zero-filled**: a zero count word reads as "bands are
/// off", which is a specific and wrong finding rather than a missing one.
fn parse_hex_bytes(r: &serde_json::Value, want: usize) -> Result<Vec<u8>, Refusal> {
    let s = r["bytes"].as_str().unwrap_or_default();
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes: Option<Vec<u8>> = if s.len() >= want * 2 {
        (0..want)
            .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok())
            .collect()
    } else {
        None
    };
    bytes.ok_or_else(|| {
        Refusal::local(format!(
            "the window could not read {want} bytes: emulator/read_memory answered {:?}, and a short \
             read decoded as zeroes would report bands as off rather than as unread",
            r["bytes"]
        ))
    })
}

// -------------------------------------------------------------------------------------------------------
// The panel's own state, and the pause discipline every gesture runs under
// -------------------------------------------------------------------------------------------------------

/// **What the window paused the machine to do here.**
///
/// [`crate::spawn_picker::Deed`]'s two clauses, supplied by this panel because the states are shared and
/// the sentences must not be: *"paused the machine to place the object"* is a false account of an effect
/// swap, and a person reading it goes looking for an object that was never placed.
pub const SWITCHING: crate::spawn_picker::Deed = crate::spawn_picker::Deed {
    doing: "change the effect",
    occasion: "when you chose it",
};

/// **The standing answer to one gesture.** Head, then the cells, then whether it was refused.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Readout {
    /// The sentence. A refusal's is the server's own words plus this window's remedy.
    pub head: String,
    /// One line per cell written, in order. Empty on a refusal, which is how a reader tells "nothing
    /// happened" from "some of it did".
    pub cells: Vec<String>,
    /// Colours the readout, and is never inferred from the shape of the sentence (P5).
    pub refused: bool,
}

/// **The switchboard's whole state**, and the only thing in this crate holding a selection made here.
///
/// ⚑ **No `Serialize`, no storage key, no path.** The owner's scope is explicit — *"not meant to be
/// permanent, just testing stuff"* — and [`crate::layout`] persists `DockState<Tab>` alone. A selection
/// that came back after a restart would silently re-assert an override on a machine nobody had told,
/// which is the standing statement's whole failure mode arriving through persistence.
#[derive(Clone, Debug)]
pub struct Panel {
    /// [`Channel::key`] of the channel being looked at.
    channel: &'static str,
    /// What is typed in the prefix box. Seeded from [`Channel::prefix`] and editable, because a prefix
    /// fixed in this crate is an assertion about somebody else's game.
    prefix: String,
    /// What is typed in the filter box.
    filter: String,
    /// The last search's names, and the `total` it reported.
    names: Vec<String>,
    total: usize,
    /// Whether a search has been run for the current channel at all, so "no names" and "not looked yet"
    /// are different states rather than one empty list.
    searched: bool,
    /// ⚑ **What this panel has changed and not put back.** The source of both the standing statement and
    /// each channel's selected row: the selection *is* the change in effect, so the two cannot disagree.
    changes: Vec<Change>,
    /// The last gesture's answer.
    last: Option<Readout>,
    /// What the window did to the run state for it.
    run: Option<crate::spawn_picker::RunState>,
    /// The last readback of the current channel's live cell.
    live: Option<Result<Live, String>>,
    /// The last band-table readback.
    band_read: Option<Result<Bands, String>>,
}

impl Default for Panel {
    fn default() -> Self {
        Panel {
            channel: PARALLAX.key,
            prefix: PARALLAX.prefix.to_string(),
            filter: String::new(),
            names: Vec::new(),
            total: 0,
            searched: false,
            changes: Vec::new(),
            last: None,
            run: None,
            live: None,
            band_read: None,
        }
    }
}

impl Panel {
    /// The channel being looked at. Never `None`: the field only ever holds a [`CHANNELS`] key.
    pub fn channel(&self) -> Channel {
        Channel::by_key(self.channel).unwrap_or(PARALLAX)
    }

    /// Look at a different channel. Clears the search and seeds the prefix, because a list of scenes
    /// under a raster heading is a picker offering a choice it cannot make.
    pub fn look_at(&mut self, key: &str) {
        let Some(c) = Channel::by_key(key) else {
            return;
        };
        if self.channel == c.key {
            return;
        }
        self.channel = c.key;
        self.prefix = c.prefix.to_string();
        self.filter.clear();
        self.names.clear();
        self.total = 0;
        self.searched = false;
        // The readback belongs to the channel that was showing. Kept, it would be this panel drawing one
        // channel's live cell under another's name, which is the label-that-lies in miniature.
        self.live = None;
    }

    /// The prefix box.
    pub fn prefix_mut(&mut self) -> &mut String {
        &mut self.prefix
    }

    /// The filter box.
    pub fn filter_mut(&mut self) -> &mut String {
        &mut self.filter
    }

    /// The rows, projected. `None` before a search, so the panel can say *press the button* rather than
    /// draw an empty list that reads as *there is nothing*.
    pub fn listing(&self) -> Option<Listing> {
        self.searched.then(|| {
            listing(
                &self.channel(),
                &self.names,
                self.selected(),
                self.total,
                &self.filter,
            )
        })
    }

    /// What this panel last pointed the current channel at, or `None`.
    pub fn selected(&self) -> Option<&str> {
        let key = self.channel;
        self.changes
            .iter()
            .find(|c| c.key == key)
            .map(|c| c.target.as_str())
    }

    /// ⚑ [`statement`] over everything this panel has changed. The standing line.
    pub fn statement(&self) -> Option<String> {
        statement(&self.changes)
    }

    /// The last gesture's answer.
    pub fn last(&self) -> Option<&Readout> {
        self.last.as_ref()
    }

    /// The standing account of what the window did to the run state, in this panel's own words.
    pub fn run_line(&self) -> Option<String> {
        self.run.as_ref().map(|r| r.sentence_of(SWITCHING))
    }

    /// Whether that account is the alarming kind, for the renderer to colour on (P5).
    pub fn run_alarming(&self) -> bool {
        self.run
            .as_ref()
            .is_some_and(crate::spawn_picker::RunState::alarming)
    }

    /// The last live-cell readback: the line, or the refusal that replaced it.
    pub fn live_line(&self) -> Option<Result<String, String>> {
        let channel = self.channel();
        self.live
            .as_ref()
            .map(|r| r.as_ref().map(|l| l.line(&channel)).map_err(String::clone))
    }

    /// The last band readback.
    pub fn band_read(&self) -> Option<Result<&Bands, &String>> {
        self.band_read.as_ref().map(Result::as_ref)
    }

    // ---------------------------------------------------------------------------------------------
    // The gestures
    // ---------------------------------------------------------------------------------------------

    /// **Run the prefix search.** A pure read: no pause, nothing to restore.
    pub fn search(&mut self, machine: &mut crate::machine::Machine, bus: &mut crate::bus::Bus) {
        let prefix = self.prefix.trim().to_string();
        if prefix.is_empty() {
            self.last = Some(Readout {
                head: "the prefix box is empty, so there is nothing to search for. An empty prefix \
                       would ask the bus for every symbol in the listing and be cut off at its bound, \
                       which is a list that looks like an answer."
                    .to_string(),
                cells: Vec::new(),
                refused: true,
            });
            return;
        }
        let sys = machine.system_mut();
        let mut c = crate::screen_pick::PlayerCaller { bus, sys };
        match candidates(&mut c, &prefix) {
            Ok((names, total)) => {
                self.names = names;
                self.total = total;
                self.searched = true;
                self.last = Some(Readout {
                    head: format!(
                        "{} under {prefix:?}, out of {total} the listing holds under it.",
                        plural(self.names.len(), "name", "names")
                    ),
                    cells: Vec::new(),
                    refused: false,
                });
            }
            Err(e) => {
                self.searched = false;
                self.names.clear();
                self.total = 0;
                self.last = Some(Readout {
                    head: refusal_line(&e, &format!("search under {prefix:?}")),
                    cells: Vec::new(),
                    refused: true,
                });
            }
        }
    }

    /// **Point the current channel at `name`.** Pause, write the whole set, resume.
    pub fn select(
        &mut self,
        machine: &mut crate::machine::Machine,
        bus: &mut crate::bus::Bus,
        name: &str,
    ) {
        let channel = self.channel();
        let what = format!("point the {} at {name}", channel.title);
        self.gesture(machine, bus, &what, |c| point_at(c, &channel, name));
    }

    /// **Turn the current channel off**, or say why it cannot be.
    pub fn off(&mut self, machine: &mut crate::machine::Machine, bus: &mut crate::bus::Bus) {
        let channel = self.channel();
        let what = format!("turn the {} off", channel.title);
        self.gesture(machine, bus, &what, |c| turn_off(c, &channel));
    }

    /// ⚑ **The one pause-write-resume path**, shared by every gesture that writes.
    ///
    /// `paused_for` is **called, never re-implemented**: *"the machine really was paused while the body
    /// ran"* is the property the design rests on, and this crate keeps one implementation of it so a
    /// second surface cannot capture and restore differently. It is also what makes
    /// **already-paused stays paused** true here for free — resuming a machine the person deliberately
    /// stopped is a state change they did not ask for, and that decision lives in one place.
    fn gesture(
        &mut self,
        machine: &mut crate::machine::Machine,
        bus: &mut crate::bus::Bus,
        what: &str,
        body: impl FnOnce(&mut crate::screen_pick::PlayerCaller<'_>) -> Result<Wrote, Refusal>,
    ) {
        let channel = self.channel();
        let outcome = crate::screen_pick::paused_for(machine, bus, |machine, bus| {
            let sys = machine.system_mut();
            body(&mut crate::screen_pick::PlayerCaller { bus, sys })
        });
        let (wrote, run) = match outcome {
            Ok(both) => both,
            // The window never got as far as touching the run state, so there is nothing to restore and
            // nothing to say about it.
            Err(why) => {
                self.run = None;
                self.last = Some(Readout {
                    head: format!(
                        "the window could not pause the machine to {what}, so nothing was written. \
                         {why}"
                    ),
                    cells: Vec::new(),
                    refused: true,
                });
                return;
            }
        };
        self.run = Some(run);
        self.last = Some(match wrote {
            Ok(w) => {
                record(
                    &mut self.changes,
                    Change {
                        key: channel.key.to_string(),
                        title: channel.title.to_string(),
                        target: w.target.clone(),
                    },
                );
                Readout {
                    head: w.line(),
                    cells: w.cells,
                    refused: false,
                }
            }
            // ⚑ A refusal does NOT record a change. The standing statement names what is in effect, and a
            // write that was refused is not in effect: recording it would make the loudest line on this
            // panel the one that is wrong.
            Err(e) => Readout {
                head: refusal_line(&e, what),
                cells: Vec::new(),
                refused: true,
            },
        });
        // A readback is stale the instant a write lands, and a stale one beside a fresh selection is
        // exactly the picture this panel exists to prevent. **Retired rather than retaken**: a retake is
        // a read the person did not ask for, and on the raster channel it would name the OLD program
        // after every successful selection (the swap happens at the next `Raster_VBlank`), which reads
        // as a failed write.
        self.live = None;
        self.band_read = None;
    }

    /// **Read the current channel's live cell back.** A pure read: no pause.
    pub fn refresh_live(
        &mut self,
        machine: &mut crate::machine::Machine,
        bus: &mut crate::bus::Bus,
    ) {
        let channel = self.channel();
        let sys = machine.system_mut();
        let mut c = crate::screen_pick::PlayerCaller { bus, sys };
        self.live =
            Some(live(&mut c, &channel).map_err(|e| refusal_line(&e, "read the live cell")));
    }

    /// **Read the band table back.** A pure read: no pause.
    pub fn refresh_bands(
        &mut self,
        machine: &mut crate::machine::Machine,
        bus: &mut crate::bus::Bus,
    ) {
        let sys = machine.system_mut();
        let mut c = crate::screen_pick::PlayerCaller { bus, sys };
        self.band_read =
            Some(read_bands(&mut c).map_err(|e| refusal_line(&e, "read the band table")));
    }
}

/// **A refusal, as one line**, saying whose refusal it is and carrying the server's own words.
///
/// [`oracle_frontend::spawn::Refusal::terminal`]'s shape, re-spelled here for one reason: that one is
/// written around a *spawn* — it says *"nothing was placed"* and names an archetype — and printing it
/// over an effect swap would describe a gesture nobody made. The parts are the same fields, the message
/// is carried verbatim, and the remedy comes from the same reason-keyed table with this window's own
/// pause key.
fn refusal_line(e: &Refusal, what: &str) -> String {
    let who = match (e.code, e.reason.as_deref()) {
        (Some(c), Some(r)) => format!("aether {c} {r}"),
        (Some(c), None) => format!("aether {c}"),
        (None, _) => "the window".to_string(),
    };
    let mut s = format!(
        "refused by {who}, nothing changed, could not {what}: {}",
        e.message
    );
    if let Some(r) = e.remedy(Some(&crate::screen_pick::pause_remedy())) {
        s.push_str(&format!(". {r}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    // ---------------------------------------------------------------------------------------------
    // A caller that answers like the bus and remembers what it was asked
    // ---------------------------------------------------------------------------------------------

    /// **A stand-in for the bus**, so the choreography can be checked against values a test chooses.
    ///
    /// It is deliberately not a mock of the *engine*: it answers `lookup_symbol` out of a listing the
    /// test writes and records every call. What the gates below assert is **which cells were written,
    /// with which values, addressed how** — which is the whole of what this module decides and the whole
    /// of what a wrong write-set gets wrong.
    struct Fake {
        /// `(name, addr, raw_addr)`. The two spellings differ for work-RAM symbols exactly as the bus's
        /// do, so a test can tell an `addr` from a `rawAddr` in a written value.
        listing: Vec<(&'static str, u32, u32)>,
        /// Every call, in order.
        calls: Vec<(String, Value)>,
        /// `(symbol_or_addr, hex_bytes)` served to `read_memory`.
        reads: Vec<(String, String)>,
        /// A symbol whose write is refused, and the refusal.
        refuse_write: Option<(&'static str, i64, &'static str)>,
    }

    impl Fake {
        fn new(listing: Vec<(&'static str, u32, u32)>) -> Self {
            Fake {
                listing,
                calls: Vec::new(),
                reads: Vec::new(),
                refuse_write: None,
            }
        }

        /// Every symbol this module's three channels touch, at the addresses both listings measured on
        /// this box actually carry. **The two debug-only ones are present here**; the tests that are
        /// about their absence take them out rather than the other tests inventing them.
        fn full() -> Self {
            Fake::new(vec![
                ("Parallax_Current_Config", 0xFF_88EC, 0xFFFF_88EC),
                ("Parallax_Target_Config", 0xFF_88F0, 0xFFFF_88F0),
                ("Parallax_Transition_Frames", 0xFF_88F4, 0xFFFF_88F4),
                ("Parallax_Snap_Pending", 0xFF_88F5, 0xFFFF_88F5),
                ("ParallaxConfig_Haze", 0x01_2C6C, 0x0001_2C6C),
                ("ParallaxConfig_OJZ_Default", 0x01_267A, 0x0001_267A),
                ("Raster_Program", 0xFF_8BD6, 0xFFFF_8BD6),
                ("Raster_Pending", 0xFF_8BDE, 0xFFFF_8BDE),
                ("Raster_Program_None", 0x00_881E, 0x0000_881E),
                ("EditorRaster_OJZ_Act1_ramp_probe", 0x01_4652, 0x0001_4652),
                ("BgAnim_Table_Ptr", 0xFF_E91A, 0xFFFF_E91A),
                ("BgAnim_LastStep", 0xFF_8F06, 0xFFFF_8F06),
                ("BgAnim_Table", 0x02_8BD4, 0x0002_8BD4),
                ("Debug_Lab_Index", 0xFF_EE0D, 0xFFFF_EE0D),
            ])
        }

        fn without(mut self, name: &str) -> Self {
            self.listing.retain(|(n, _, _)| *n != name);
            self
        }

        fn serving(mut self, key: &str, bytes: &str) -> Self {
            self.reads.push((key.to_string(), bytes.to_string()));
            self
        }

        /// The `emulator/write_memory` calls, as `(symbol, disp, value, width)`.
        fn writes(&self) -> Vec<(String, u64, u64, u64)> {
            self.calls
                .iter()
                .filter(|(m, _)| m == "emulator/write_memory")
                .map(|(_, p)| {
                    (
                        p["symbol"]
                            .as_str()
                            .unwrap_or("<no symbol key>")
                            .to_string(),
                        p["disp"].as_u64().unwrap_or(0),
                        p["value"].as_u64().unwrap_or(u64::MAX),
                        p["width"].as_u64().unwrap_or(0),
                    )
                })
                .collect()
        }
    }

    impl Caller for Fake {
        fn call(&mut self, method: &str, params: Value) -> Result<Value, Refusal> {
            self.calls.push((method.to_string(), params.clone()));
            match method {
                "emulator/lookup_symbol" => {
                    if let Some(q) = params["addr"].as_str() {
                        let want = u32::from_str_radix(q.trim_start_matches("0x"), 16).unwrap();
                        // Nearest preceding, as the bus answers the address direction.
                        let hit = self
                            .listing
                            .iter()
                            .filter(|(_, a, _)| *a <= want)
                            .max_by_key(|(_, a, _)| *a);
                        return Ok(match hit {
                            Some((n, a, _)) => json!({"name": n, "disp": want - a}),
                            None => json!({}),
                        });
                    }
                    let name = params["name"].as_str().unwrap_or_default();
                    if let Some((n, a, r)) = self.listing.iter().find(|(n, _, _)| *n == name) {
                        return Ok(json!({
                            "name": n,
                            "addr": format!("0x{a:06X}"),
                            "rawAddr": format!("0x{r:08X}"),
                            "exact": true,
                        }));
                    }
                    let items: Vec<Value> = self
                        .listing
                        .iter()
                        .filter(|(n, _, _)| n.starts_with(name))
                        .map(|(n, a, _)| json!({"name": n, "addr": format!("0x{a:06X}")}))
                        .collect();
                    let total = items.len();
                    Ok(json!({"exact": false, "otherMatches": {"items": items, "total": total}}))
                }
                "emulator/write_memory" => {
                    let sym = params["symbol"].as_str().unwrap_or_default();
                    if let Some((s, code, msg)) = self.refuse_write {
                        if s == sym {
                            return Err(Refusal {
                                code: Some(code),
                                reason: None,
                                message: msg.to_string(),
                                remedy: None,
                            });
                        }
                    }
                    Ok(json!({"addr": "0x00FF0000", "len": params["width"]}))
                }
                "emulator/read_memory" => {
                    let key = params["symbol"]
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| params["addr"].as_str().map(str::to_string))
                        .unwrap_or_default();
                    match self.reads.iter().find(|(k, _)| *k == key) {
                        Some((_, b)) => Ok(json!({"bytes": b})),
                        None => Err(Refusal::local(format!(
                            "this fake serves no bytes at {key}"
                        ))),
                    }
                }
                other => Err(Refusal::local(format!("this fake serves no {other}"))),
            }
        }

        fn address_of(&mut self, symbol: &str) -> Option<u32> {
            self.listing
                .iter()
                .find(|(n, _, _)| *n == symbol)
                .map(|(_, a, _)| *a)
        }
    }

    // ---------------------------------------------------------------------------------------------
    // ⚑ The write-sets, transcribed and pinned
    // ---------------------------------------------------------------------------------------------

    /// ⚑ **THE TRANSCRIPTION CHECK.** The cells each channel writes, spelled out here against the line
    /// of aeon's engine each came from.
    ///
    /// # Why this is a literal table and not derived from [`CHANNELS`]
    ///
    /// Every other gate below runs the real choreography and compares against `channel.writes`, which
    /// checks the **plumbing** — order, symbol addressing, displacement, width, the value that travels —
    /// and is blind by construction to the table itself being wrong. That blindness is the expensive one:
    /// the card this parcel was written from would have produced a one-cell raster set that passes every
    /// derived assertion and names the new program while the screen draws the old one.
    ///
    /// So this row is the second source. It is the shape [`crate::layout`]'s `VOCABULARIES` argues for
    /// over an `assert_eq!` on a number: the citation rides the expectation, so whoever changes the table
    /// has to change this and lands on the line of `engine/` they are contradicting while doing it.
    ///
    /// **Values are given as [`Put`]**, so `Put::Target` is asserted as *the target's address travels
    /// here* rather than as a number a test chose.
    /// One row of [`transcribed`], named rather than a five-tuple so the `why` cannot silently swap
    /// places with the symbol when somebody adds a field.
    struct Wanted {
        symbol: &'static str,
        disp: u32,
        width: u8,
        put: Put,
        /// The line of aeon's engine this cell came from, and what goes wrong without it. It rides the
        /// expectation so a change to the table lands the author on the line they are contradicting.
        why: &'static str,
    }

    fn transcribed() -> Vec<(&'static str, Vec<Wanted>)> {
        vec![
            (
                "parallax",
                vec![
                    Wanted {
                        symbol: "Parallax_Current_Config",
                        disp: 0,
                        width: 4,
                        put: Put::Target,
                        why: "parallax.emp:1284, Parallax_StartTransition's instant arm",
                    },
                    Wanted {
                        symbol: "Parallax_Target_Config",
                        disp: 0,
                        width: 4,
                        put: Put::Lit(0),
                        why: "parallax.emp:1285. Without it Parallax_Update promotes the staged target over \
                         the write when its counter expires (:1655-1657)",
                    },
                    Wanted {
                        symbol: "Parallax_Transition_Frames",
                        disp: 0,
                        width: 1,
                        put: Put::Lit(0),
                        why: "parallax.emp:1286. Without it Parallax_Update drives from the staged target and \
                         ignores Current_Config (:1650)",
                    },
                    Wanted {
                        symbol: "Parallax_Snap_Pending",
                        disp: 0,
                        width: 1,
                        put: Put::Lit(1),
                        why: "parallax.emp:1287",
                    },
                ],
            ),
            (
                "raster",
                vec![Wanted {
                        symbol: "Raster_Pending",
                        disp: 0,
                        width: 4,
                        put: Put::Target,
                        why: "raster.emp:946, the WHOLE of Raster_Install. NOT Raster_Program, which is what \
                     Raster_VBlank writes on the way in: the HInt walker reads Raster_Active_Buf \
                     (:1052), so writing the program cell names a swap the screen never makes",
                    }],
            ),
            (
                "bands",
                vec![
                    Wanted {
                        symbol: "BgAnim_Table_Ptr",
                        disp: 0,
                        width: 4,
                        put: Put::Target,
                        why: "bg_anim.emp:183",
                    },
                    Wanted {
                        symbol: "BgAnim_LastStep",
                        disp: 0,
                        width: 4,
                        put: Put::Lit(0xFFFF_FFFF),
                        why: "bg_anim.emp:185, the init sentinel. Without it a band on the outgoing table's \
                         step takes .skip_band forever and the new table never paints",
                    },
                    Wanted {
                        symbol: "BgAnim_LastStep",
                        disp: 4,
                        width: 4,
                        put: Put::Lit(0xFFFF_FFFF),
                        why: "bg_anim.emp:186, the second half of the same eight-byte array",
                    },
                ],
            ),
        ]
    }

    #[test]
    fn every_channels_write_set_is_the_installers_own_cells() {
        // Loud on unmeasurable: a table that stopped covering the channels is a vacuous gate.
        assert_eq!(
            transcribed().len(),
            CHANNELS.len(),
            "the transcription table and CHANNELS have drifted apart, so this gate covers less than it \
             claims to"
        );
        for (key, want) in transcribed() {
            let c = Channel::by_key(key).unwrap_or_else(|| panic!("no channel {key:?}"));
            let got: Vec<_> = c
                .writes
                .iter()
                .map(|w| (w.symbol, w.disp, w.width, w.put))
                .collect();
            let expect: Vec<_> = want
                .iter()
                .map(|w| (w.symbol, w.disp, w.width, w.put))
                .collect();
            assert_eq!(
                got,
                expect,
                "the {key} write-set is not the installer's. The cells it must have, and why:\n{}",
                want.iter()
                    .map(|w| format!("  {}+{} width {}: {}", w.symbol, w.disp, w.width, w.why))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }

    /// ⚑ **Exactly one cell per channel carries the target**, and it is the pointer.
    ///
    /// Derived from the table rather than pinned: a channel with two `Put::Target` cells would write the
    /// same address into two places, and one with none would run a whole write-set that never mentions
    /// what the person picked.
    #[test]
    fn exactly_one_cell_of_each_write_set_carries_what_was_picked() {
        for c in CHANNELS {
            let n = c.writes.iter().filter(|w| w.put == Put::Target).count();
            assert_eq!(
                n, 1,
                "{} writes {n} target cells; a write-set with none never mentions the selection and one \
                 with two puts it in a place the installer does not",
                c.key
            );
        }
    }

    // ---------------------------------------------------------------------------------------------
    // ⚑ Debug_Lab_Index, guarded on two independent routes
    // ---------------------------------------------------------------------------------------------

    /// ⚑ **The cursor is refused by NAME and by ADDRESS, and neither check covers the other.**
    ///
    /// The two routes in are different mistakes: a write-set edited to name the cursor, and a cell whose
    /// symbol happens to resolve at the cursor's address in some build. A guard that only checked names
    /// would poke `$FFFFEE0D` through a symbol called something else.
    ///
    /// The control is the third assertion: a real selector at a real address passes, so the two above
    /// are refusing this address rather than refusing everything.
    #[test]
    fn the_lab_index_is_refused_by_name_and_by_address_independently() {
        let by_name = forbidden(LAB_INDEX_SYMBOL, 0x00FF_0000)
            .expect("the cursor's NAME must be refused even at an unrelated address");
        assert!(
            by_name.message.contains("named as the write target"),
            "the refusal must say which route it caught: {}",
            by_name.message
        );

        let by_addr = forbidden("Something_Else_Entirely", LAB_INDEX_ADDR)
            .expect("the cursor's ADDRESS must be refused even under another name");
        assert!(
            by_addr.message.contains("resolves to that address"),
            "the refusal must say which route it caught: {}",
            by_addr.message
        );

        for r in [&by_name, &by_addr] {
            assert_eq!(r.reason.as_deref(), Some("labIndexIsNotASelector"));
            assert!(
                r.message.contains("changes nothing that runs"),
                "the refusal must say WHY, not merely that it refused: {}",
                r.message
            );
        }

        // The control. Without it both rows above pass on a `forbidden` that refuses everything.
        assert_eq!(
            forbidden(PARALLAX.selector, 0xFF_88EC),
            None,
            "a real selector at a real address must pass, or the two rows above witness nothing"
        );
    }

    /// ⚑ **The ADDRESS route fires on a symbol the bus resolved, not only on the constant.**
    ///
    /// # This row exists because the guard it checks was dead, and its own sibling could not tell
    ///
    /// `the_lab_index_is_refused_by_name_and_by_address_independently` hands [`forbidden`] the constant
    /// and passes. For a while the shipped path handed it the **24-bit door address** instead, and
    /// `$FFEE0D` never equals `$FFFFEE0D`, so the address branch could not fire on any real input while
    /// that sibling went on being green. The two spellings are the whole bug, so this row goes through
    /// `run` with a real listing and a real resolve, which is the only arrangement that can see it.
    ///
    /// The mutation lesson banked in this repo, arriving again: a guard tested on the value it was
    /// written against is tested on the one input that cannot expose it.
    #[test]
    fn the_address_route_fires_on_a_resolved_symbol_and_not_only_on_the_constant() {
        // A channel whose cell is named innocently and sits exactly where the cursor sits. Nothing in
        // the name check can see this, which is the point of having two.
        const TRAP: Channel = Channel {
            key: "trap",
            title: "trap",
            selector: "Parallax_Current_Config",
            noted_addr: 0xFFFF_88EC,
            installer: "a channel table somebody edited without reading the header",
            writes: &[Cell {
                symbol: "Some_Other_Cell",
                disp: 0,
                width: 1,
                put: Put::Lit(1),
                why: "a cell that resolves onto the START chord's cursor",
            }],
            prefix: "ParallaxConfig_",
            debug_only: false,
            off: Off::No("n/a"),
            zero: "n/a",
            subject: "n/a",
        };

        let mut f = Fake::full();
        f.listing
            .push(("Some_Other_Cell", 0xFF_EE0D, LAB_INDEX_ADDR));
        let e = run(&mut f, &TRAP, "whatever", 0)
            .expect_err("a cell resolving onto the cursor must be refused");
        assert_eq!(e.reason.as_deref(), Some("labIndexIsNotASelector"));
        assert!(
            e.message.contains("resolves to that address"),
            "it must be the ADDRESS route that caught it, not the name route: {}",
            e.message
        );
        assert!(f.writes().is_empty(), "wrote {:?} anyway", f.writes());

        // ⚑ And the spelling is what makes it work. Fed the 24-bit door form the branch cannot fire,
        // which is exactly the dead guard this row was written against. Pinned so that a future change
        // routing the door address in here fails HERE, with this sentence, rather than silently.
        assert_eq!(
            forbidden("Some_Other_Cell", 0xFF_EE0D),
            None,
            "the 24-bit door form cannot match LAB_INDEX_ADDR, which is why `run` resolves and passes \
             the 32-bit `rawAddr`. If this ever starts refusing, the constant has changed spelling and \
             the doc on `forbidden` is stale"
        );
    }

    /// ⚑ **No channel's write-set can reach the cursor**, on either route, checked over the shipped table.
    #[test]
    fn no_shipped_write_set_names_or_resolves_to_the_cursor() {
        let mut checked = 0;
        for c in CHANNELS {
            for w in c.writes {
                let mut f = Fake::full();
                let (_, raw) =
                    resolve(&mut f, w.symbol).expect("the fake listing carries every cell");
                assert_eq!(
                    forbidden(w.symbol, raw.wrapping_add(w.disp)),
                    None,
                    "{}'s cell `{}` reaches the START chord's cursor",
                    c.key,
                    w.symbol
                );
                checked += 1;
            }
        }
        // Loud on unmeasurable: a listing that stopped carrying these names would make every row above
        // pass on a `resolve` that never ran.
        assert!(
            checked >= CHANNELS.len(),
            "only {checked} cells were checked across {} channels, which is fewer than one each",
            CHANNELS.len()
        );
    }

    // ---------------------------------------------------------------------------------------------
    // ⚑ The gesture, end to end against the fake
    // ---------------------------------------------------------------------------------------------

    /// ⚑ **A selection writes the installer's cells, by SYMBOL, in order, and writes nothing else.**
    ///
    /// The expectations are the channel's own write-set, which is what makes this the *plumbing* gate:
    /// it catches a cell dropped, reordered, addressed by `addr` instead of `symbol`, given the wrong
    /// width, or handed a value the target never resolved to.
    /// `every_channels_write_set_is_the_installers_own_cells` is what catches the table itself.
    ///
    /// ⚑ The value on a `Put::Target` cell is asserted to be the **`rawAddr`**, which is the spelling a
    /// pointer field holds. The fake's two spellings differ, as the bus's do for work-RAM symbols, so a
    /// build that sent the 24-bit form would fail here rather than pass on numbers that happen to agree.
    #[test]
    fn a_selection_writes_the_installers_cells_by_symbol_and_nothing_else() {
        for (channel, target, target_raw) in [
            (PARALLAX, "ParallaxConfig_Haze", 0x0001_2C6Cu64),
            (RASTER, "EditorRaster_OJZ_Act1_ramp_probe", 0x0001_4652),
            (BANDS, "BgAnim_Table", 0x0002_8BD4),
        ] {
            let mut f = Fake::full();
            let w = point_at(&mut f, &channel, target)
                .unwrap_or_else(|e| panic!("{} refused: {}", channel.key, e.message));

            let want: Vec<(String, u64, u64, u64)> = channel
                .writes
                .iter()
                .map(|c| {
                    (
                        c.symbol.to_string(),
                        u64::from(c.disp),
                        match c.put {
                            Put::Target => target_raw,
                            Put::Lit(v) => u64::from(v),
                        },
                        u64::from(c.width),
                    )
                })
                .collect();
            assert_eq!(
                f.writes(),
                want,
                "{} did not write its installer's cells",
                channel.key
            );
            assert_eq!(w.cells.len(), channel.writes.len());

            // Not one `addr` key on any write: the destination is the server's to resolve.
            for (m, p) in &f.calls {
                if m == "emulator/write_memory" {
                    assert!(
                        p.get("addr").is_none() && p.get("symbol").is_some(),
                        "a write addressed by address rather than by name: {p}"
                    );
                }
            }
        }
    }

    /// ⚑ **The raster channel stages `Raster_Pending` and never touches `Raster_Program`.**
    ///
    /// The single correction this module carries, as its own row, because it is the one a reasonable
    /// implementation gets wrong: both cells exist, both are longwords, both are named "the raster
    /// program" in conversation, and only one of them installs anything.
    #[test]
    fn selecting_a_raster_program_stages_pending_and_never_writes_the_program_cell() {
        let mut f = Fake::full();
        point_at(&mut f, &RASTER, "EditorRaster_OJZ_Act1_ramp_probe").expect("a listed program");
        let written: Vec<String> = f.writes().into_iter().map(|(s, ..)| s).collect();
        assert_eq!(
            written,
            vec!["Raster_Pending".to_string()],
            "raster.emp:946: Raster_Install's whole body is `move.l a0, Raster_Pending`. Writing \
             Raster_Program instead names a program the HInt walker never reads, because the walker \
             reads Raster_Active_Buf (:1052) and only Raster_VBlank re-points it"
        );
        assert!(
            !written.iter().any(|s| s == RASTER.selector),
            "`{}` is what Raster_VBlank writes on the way in; this panel must not write it",
            RASTER.selector
        );
    }

    /// ⚑ **A cell refused mid-set says how far it got, and records nothing.**
    ///
    /// The half-applied installer is a real state and the honest repair is naming it. It is checked on
    /// the parallax set because that is the one with cells after the first: a set of one cannot be half
    /// applied, so testing it there would be a row that cannot fail.
    #[test]
    fn a_cell_refused_mid_set_names_how_far_it_got() {
        let mut f = Fake::full();
        f.refuse_write = Some((
            "Parallax_Transition_Frames",
            -32005,
            "the machine is free-running",
        ));
        let e = point_at(&mut f, &PARALLAX, "ParallaxConfig_Haze")
            .expect_err("a refused cell must refuse the gesture");
        assert_eq!(e.reason.as_deref(), Some("writeSetIncomplete"));
        assert!(
            e.message.contains("2 of 4") && e.message.contains("HALF SET"),
            "the refusal must say how far the set got and that the channel is half applied: {}",
            e.message
        );
        assert!(
            e.message.contains("the machine is free-running"),
            "the server's own words must survive verbatim: {}",
            e.message
        );
        // And the cells that did land are the ones before it, in order.
        assert_eq!(
            f.writes()
                .iter()
                .map(|(s, ..)| s.as_str())
                .collect::<Vec<_>>(),
            [
                "Parallax_Current_Config",
                "Parallax_Target_Config",
                "Parallax_Transition_Frames"
            ],
            "the refused cell is attempted and the ones after it are not"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // ⚑ The two absences that are rulings rather than gaps
    // ---------------------------------------------------------------------------------------------

    /// ⚑ **Bands-off is refused, in words, until `BgAnim_Table_Empty` exists.**
    ///
    /// The obvious implementation points at the act's own `BgAnim_Table` and works, on the act in front
    /// of you, because that act's count word happens to be 0. This asserts the refusal AND that nothing
    /// was written, because a refusal that had already poked something is worse than no refusal.
    #[test]
    fn bands_off_is_refused_until_the_empty_table_constant_exists_and_writes_nothing() {
        let mut f = Fake::full();
        let e =
            turn_off(&mut f, &BANDS).expect_err("there is no BgAnim_Table_Empty in this listing");
        assert_eq!(e.reason.as_deref(), Some("offTargetMissing"));
        // ⚑ **It must read as "your listing is old", not as "this is broken".** The symbol exists in
        // the engine now, and the listings on the box this was written on predate it, so this refusal
        // is the one a person will actually meet. It has to name the missing symbol, say the feature is
        // there, and give the action.
        assert!(
            e.message.contains("BgAnim_Table_Empty"),
            "the refusal must NAME the missing symbol or the reader has nothing to check: {}",
            e.message
        );
        assert!(
            e.message.contains("older than the build") || e.message.contains("EXISTS in the engine"),
            "it must say the feature is present and the listing is behind, not that bands-off does not \
             exist: {}",
            e.message
        );
        assert!(
            e.message.contains("happens to hold a zero count"),
            "and it must say why the obvious substitute is wrong, since that is what a reader reaches \
             for next: {}",
            e.message
        );
        let remedy = e
            .remedy(None)
            .expect("a refusal a rebuild fixes owes the reader that action");
        assert!(
            remedy.contains("BgAnim_Table_Empty") && remedy.contains(EMPTY_TABLE_COMMIT),
            "the remedy must name the symbol and the commit that carries it: {remedy}"
        );
        assert!(
            f.writes().is_empty(),
            "a refused off gesture wrote {:?}",
            f.writes()
        );

        // The control: once the constant is in the listing, the same gesture runs the same write-set.
        //
        // ⚑ **And the VALUE is asserted, not only the cell count.** This row used to check the target's
        // NAME and the number of writes and nothing else, which left `turn_off` free to put any number
        // at all into the selector: a mutation replacing the resolved address with `channel.noted_addr`
        // was applied and the suite stayed GREEN. The off path is the one place a value is chosen by
        // this module rather than handed to it, so it is the one place that needed saying out loud.
        // Derived from the write-set and the fake's own listing, never a literal typed twice.
        let mut f = Fake::full();
        let empty_raw = 0x0002_9000u64;
        f.listing
            .push(("BgAnim_Table_Empty", 0x02_9000, empty_raw as u32));
        let w = turn_off(&mut f, &BANDS).expect("the constant is present now");
        assert_eq!(w.target, "BgAnim_Table_Empty");
        let want: Vec<(String, u64, u64, u64)> = BANDS
            .writes
            .iter()
            .map(|c| {
                (
                    c.symbol.to_string(),
                    u64::from(c.disp),
                    match c.put {
                        Put::Target => empty_raw,
                        Put::Lit(v) => u64::from(v),
                    },
                    u64::from(c.width),
                )
            })
            .collect();
        assert_eq!(
            f.writes(),
            want,
            "off must run the whole installer with the EMPTY TABLE's own resolved address in the target \
             cell, not merely the right number of writes"
        );
    }

    /// ⚑ **Bands-off needs BOTH symbols, and the DESTINATION is checked first.**
    ///
    /// # ⚑ This is not a hypothetical. It is the shape the symbol actually shipped in
    ///
    /// `BgAnim_Table_Empty` landed at [`EMPTY_TABLE_COMMIT`], declared as
    /// `pub data BgAnim_Table_Empty: [u16; BGANIM_EMPTY_EMIT] = if DEBUG == 1 { [0] } else { [] }`.
    /// So in a **release** build the NAME enters the listing with an address while the array emits
    /// **nothing**: there is no zero word behind it there. `BgAnim_Table_Ptr` is genuinely absent from a
    /// release listing.
    ///
    /// A gate keyed on the target alone would therefore offer a bands-off button on a release build that
    /// writes **an address with no zero behind it** into **a cell that is not the band pointer**. Two
    /// wrongs in one gesture, and each looks fine on its own. `turn_off` resolves the destination first,
    /// and that ordering is the correctness property this row holds.
    #[test]
    fn bands_off_needs_the_destination_pointer_too_not_only_the_empty_table() {
        // The exact shape a RELEASE listing has: the constant's name present with an address, the
        // destination pointer absent. Not contrived -- it is what the declaration above produces.
        let mut f = Fake::full().without(BANDS.selector);
        f.listing
            .push(("BgAnim_Table_Empty", 0x02_9000, 0x0002_9000));
        let e = turn_off(&mut f, &BANDS)
            .expect_err("no destination pointer means no bands channel at all");
        assert_eq!(
            e.reason.as_deref(),
            Some("debugOnlyChannel"),
            "the destination must be checked BEFORE the off target, or a release build gets an off              button pointing at RAM that is not the selector"
        );
        assert!(f.writes().is_empty(), "wrote {:?} anyway", f.writes());

        // The control, which is what makes the row above about the ORDER rather than about refusing
        // whenever anything is missing: with the destination present and the constant absent, the
        // refusal is the OTHER one.
        let mut f = Fake::full();
        let e = turn_off(&mut f, &BANDS).expect_err("no constant");
        assert_eq!(e.reason.as_deref(), Some("offTargetMissing"));
    }

    /// **A scene has no off state, and the refusal says so rather than doing nothing.**
    #[test]
    fn a_scene_cannot_be_turned_off_and_the_control_says_why() {
        let mut f = Fake::full();
        let e = turn_off(&mut f, &PARALLAX).expect_err("a scene is always in effect");
        assert_eq!(e.reason.as_deref(), Some("noOffState"));
        assert!(e.message.contains("always in effect"), "{}", e.message);
        assert!(f.writes().is_empty());

        // The control: the raster channel's off IS available, so the row above is about parallax rather
        // than about `turn_off` refusing everything. ⚑ Its VALUE is checked for the reason the band
        // control's is: the off path chooses a value rather than being handed one.
        let mut f = Fake::full();
        let w = turn_off(&mut f, &RASTER).expect("Raster_Program_None is in this listing");
        assert_eq!(w.target, "Raster_Program_None");
        let (_, none_raw) = resolve(&mut f, "Raster_Program_None").expect("in the fake listing");
        assert_eq!(
            f.writes(),
            vec![(
                RASTER.writes[0].symbol.to_string(),
                0,
                u64::from(none_raw),
                u64::from(RASTER.writes[0].width),
            )],
            "raster off must stage the EMPTY PROGRAM's own resolved address"
        );
    }

    /// ⚑ **The band channel is gated on the SYMBOL, and the refusal says it is a debug-build channel.**
    ///
    /// Never on an address: on a release build `$FFFFE91A` is something else entirely, and a panel keyed
    /// on the number writes into unrelated RAM with no fault to show for it. The control is the second
    /// half: with the same symbol missing, a channel that is *not* debug-only gets the ordinary
    /// not-in-listing refusal, so this is reading `debug_only` rather than reporting every absence the
    /// same way.
    #[test]
    fn the_band_channel_is_gated_on_its_symbol_and_says_it_is_debug_only() {
        let mut f = Fake::full().without(BANDS.selector);
        let e = available(&mut f, &BANDS).expect_err("no selector, no channel");
        assert_eq!(e.reason.as_deref(), Some("debugOnlyChannel"));
        assert!(
            e.message.contains("DEBUG") && e.message.contains(BANDS.selector),
            "the refusal must name the symbol and the shape: {}",
            e.message
        );

        let mut f = Fake::full().without(PARALLAX.selector);
        let e = available(&mut f, &PARALLAX).expect_err("no selector, no channel");
        assert_eq!(
            e.reason.as_deref(),
            Some("notInListing"),
            "a channel that is not debug-only must not claim to be"
        );

        // The control: with the symbol present, both are available.
        for c in [PARALLAX, BANDS] {
            let mut f = Fake::full();
            assert!(
                available(&mut f, &c).is_ok(),
                "{} should be available",
                c.key
            );
        }
    }

    // ---------------------------------------------------------------------------------------------
    // ⚑ Drift: the listing and the note disagreeing is a refusal, not a caveat
    // ---------------------------------------------------------------------------------------------

    /// ⚑ **A moved selector refuses and names BOTH addresses**, and a matching one says nothing.
    ///
    /// The expectation is [`Channel::noted_addr`] itself rather than a number typed here, so the row
    /// cannot drift from the table. What it pins is the *behaviour*: refuse rather than pick a side.
    #[test]
    fn a_selector_that_moved_refuses_and_names_both_addresses() {
        for c in CHANNELS {
            assert_eq!(
                c.drift(c.noted_addr),
                None,
                "{}: the note's own address must not read as drift",
                c.key
            );
            let moved = c.noted_addr ^ 0x10;
            let r = c
                .drift(moved)
                .unwrap_or_else(|| panic!("{}: a moved selector must refuse", c.key));
            assert_eq!(r.reason.as_deref(), Some("selectorMoved"));
            assert!(
                r.message.contains(&format!("{moved:#010X}"))
                    && r.message.contains(&format!("{:#010X}", c.noted_addr)),
                "both addresses must be in the refusal or there is nothing to check: {}",
                r.message
            );
            assert!(
                r.message.contains("Nothing was written"),
                "the refusal must say nothing happened: {}",
                r.message
            );
        }
    }

    /// **A drifted selector stops the gesture before any cell is written.**
    #[test]
    fn a_drifted_selector_stops_the_gesture_before_the_first_cell() {
        let mut f = Fake::full();
        // Move the live cell and nothing else, which is the shape of a listing built from another ROM.
        for e in f.listing.iter_mut() {
            if e.0 == PARALLAX.selector {
                e.2 = 0xFFFF_9000;
            }
        }
        let e = point_at(&mut f, &PARALLAX, "ParallaxConfig_Haze").expect_err("moved");
        assert_eq!(e.reason.as_deref(), Some("selectorMoved"));
        assert!(f.writes().is_empty(), "wrote {:?} anyway", f.writes());
    }

    // ---------------------------------------------------------------------------------------------
    // ⚑ The listing: truncation, absence, and the row that must not be selectable
    // ---------------------------------------------------------------------------------------------

    /// ⚑ **A cut-short search says so, and a whole one does not.**
    ///
    /// A truncated list reads exactly like a complete one, which is the whole reason this line exists.
    /// The numbers in the sentence are asserted because "some were hidden" without saying how many is a
    /// warning a reader cannot act on.
    #[test]
    fn a_cut_short_search_says_so_and_a_whole_one_says_nothing() {
        assert_eq!(truncation(6, 6), None, "a whole list must claim nothing");
        assert_eq!(
            truncation(6, 0),
            None,
            "a total below the count is not a cut"
        );
        let t = truncation(256, 301).expect("256 of 301 is a cut list");
        assert!(
            t.contains("256") && t.contains("301"),
            "the line must carry both numbers: {t}"
        );

        // And through the projection, over the cap the engine actually ships. Derived from the engine's
        // own config rather than typed, so a cap change cannot leave this row pinning a stale one.
        let cap = oracle_aether::engine::EngineConfig::default().max_symbol_matches;
        let names: Vec<String> = (0..cap).map(|i| format!("ParallaxConfig_{i}")).collect();
        let l = listing(&PARALLAX, &names, None, cap + 45, "");
        let t = l.truncation.expect("a search cut at the cap must say so");
        assert!(
            t.contains(&cap.to_string()) && t.contains(&(cap + 45).to_string()),
            "{t}"
        );

        // The control: the same list, whole, says nothing. Without it the row above passes on a
        // `truncation` that fires unconditionally.
        assert_eq!(listing(&PARALLAX, &names, None, cap, "").truncation, None);
    }

    /// ⚑ **A row naming one of the channel's own cells is DRAWN and NOT selectable**, with the reason.
    ///
    /// `BgAnim_Table` and `BgAnim_Table_Ptr` share a prefix, so the search that finds the act's table
    /// also finds the selector pointing at it. Pointing a selector at itself makes the engine read the
    /// pointer's own bytes as the thing it points to.
    #[test]
    fn a_row_naming_the_channels_own_cell_is_drawn_and_not_offered() {
        let names = vec!["BgAnim_Table".to_string(), BANDS.selector.to_string()];
        let l = listing(&BANDS, &names, None, 2, "");
        assert_eq!(l.rows.len(), 2, "the row is drawn, not dropped");
        let me = l
            .rows
            .iter()
            .find(|r| r.name == BANDS.selector)
            .expect("drawn");
        assert!(!me.offered, "the channel's own cell must not be selectable");
        assert!(
            me.note.as_deref().unwrap_or_default().contains("own cells"),
            "the row must carry its reason: {:?}",
            me.note
        );
        // The control: the act's own table beside it IS offered, so this is refusing one row rather than
        // refusing the list.
        let other = l
            .rows
            .iter()
            .find(|r| r.name == "BgAnim_Table")
            .expect("drawn");
        assert!(other.offered && other.note.is_none());
    }

    /// **An empty result is a sentence, not an empty box** (P6), and the two empties are different
    /// findings.
    #[test]
    fn the_two_empty_lists_are_different_findings_and_neither_is_a_blank() {
        let names = vec!["ParallaxConfig_Haze".to_string()];
        let filtered = listing(&PARALLAX, &names, None, 1, "zzz");
        let a = filtered.absence.expect("a filtered-out list owes a line");
        assert!(
            a.contains("zzz") && a.contains('1'),
            "it must name what was typed and how many it was matched against: {a}"
        );

        let nothing = listing(&PARALLAX, &[], None, 0, "");
        let b = nothing.absence.expect("an empty search owes a line too");
        assert_ne!(a, b, "'no match' and 'nothing found' must not read alike");
        assert!(b.contains("prefix"), "{b}");
    }

    // ---------------------------------------------------------------------------------------------
    // ⚑ The bands, decoded
    // ---------------------------------------------------------------------------------------------

    /// ⚑ **The record is the note's 44 bytes, and the fields only have to FIT.**
    ///
    /// The size is [`NOTE`] §2's stated `ensure`, not a sum of the fields below it. Six `u16` and eight
    /// `u32` sum to 44 and that agreement is a check rather than the derivation: a struct with tail
    /// padding, or one field mis-transcribed, sums to 44 just as readily while the walk strides
    /// differently.
    #[test]
    fn the_record_size_is_the_notes_figure_and_the_fields_only_have_to_fit() {
        assert_eq!(
            BAND_RECORD_BYTES, 44,
            "NOTE section 2: `struct bganim_band`, 44 bytes, pinned by an ensure in \
             engine/level/bg_anim.emp"
        );
        let fields = 6 * 2 + 8 * 4;
        assert!(
            fields <= BAND_RECORD_BYTES,
            "the fields transcribed here need {fields} bytes and the record is only \
             {BAND_RECORD_BYTES}, so one of the two is wrong"
        );
        assert_eq!(MAX_BANDS, 4, "NOTE section 2: BGANIM_MAX_BANDS = 4");
    }

    /// ⚑ **The note's own worked example decodes to the note's own sentence.**
    ///
    /// [`NOTE`] §2 gives the shipped act's band 0 as `[0, 4, 63, 7, 32, $8000]` and reads it back as
    /// *"Camera_X driven, 1 px per 16 units, 64 px period, rotation unit 128 B, 32 tiles, first slot at
    /// VRAM $8000"*. Every number in the expectation is **the note's**, derived by the note from the
    /// header, so this is a transcription check and not a measurement copied out of a run.
    ///
    /// The two derivations are what it is really pinning: `step_mask` 63 is a **64** px period, and
    /// `col_shift` 7 is a **128** byte unit. A readout that printed the raw numbers would be handing a
    /// person two conversions, and the off-by-one in the first is the kind that gets done wrong.
    #[test]
    fn the_notes_worked_example_decodes_to_the_notes_own_sentence() {
        let mut raw = vec![0u8; BAND_RECORD_BYTES];
        for (i, v) in [0u16, 4, 63, 7, 32, 0x8000].iter().enumerate() {
            raw[i * 2..i * 2 + 2].copy_from_slice(&v.to_be_bytes());
        }
        let b = Band::parse(&raw).expect("a whole record");
        assert_eq!(b.driver, 0);
        assert_eq!(b.rate_shift, 4);
        assert_eq!(b.step_mask, 63);
        assert_eq!(b.col_shift, 7);
        assert_eq!(b.tile_count, 32);
        assert_eq!(b.vram_dest, 0x8000);
        assert_eq!(
            b.line(),
            "Camera_X driven, 1 px per 16 units, 64 px period, rotation unit 128 B, 32 tiles, first \
             slot at VRAM $8000",
            "NOTE section 2's own reading of its own worked example"
        );

        // A driver the note does not name is said to be unknown rather than folded into one of the three.
        let mut odd = raw.clone();
        odd[0..2].copy_from_slice(&9u16.to_be_bytes());
        let l = Band::parse(&odd).unwrap().line();
        assert!(l.contains("UNKNOWN") && l.contains('9'), "{l}");

        // A short slice is None, never a record of zeroes.
        assert_eq!(Band::parse(&raw[..BAND_RECORD_BYTES - 1]), None);
    }

    /// ⚑ **A count past the ceiling is reported, not trusted**, and a zero count is a stated finding.
    #[test]
    fn a_count_the_ceiling_forbids_is_reported_and_a_zero_count_is_a_finding() {
        let record = vec![0u8; BAND_RECORD_BYTES];
        let table = |count: u16, n: usize| {
            let mut v = count.to_be_bytes().to_vec();
            for _ in 0..n {
                v.extend_from_slice(&record);
            }
            v
        };

        let too_many = bands(&table(40, MAX_BANDS));
        assert_eq!(too_many.count, 40, "the word is carried whole");
        assert_eq!(
            too_many.bands.len(),
            MAX_BANDS,
            "no more than the ceiling is decoded"
        );
        let c = too_many
            .caveat
            .expect("a count past the ceiling owes a line");
        assert!(
            c.contains("40") && c.contains(&MAX_BANDS.to_string()),
            "{c}"
        );

        // The control: a count the ceiling allows draws no caveat, so the row above is about the ceiling.
        let ok = bands(&table(2, 2));
        assert_eq!(ok.bands.len(), 2);
        assert_eq!(ok.caveat, None);
        assert_eq!(ok.absence, None);

        // Zero is bands being off, and it says so rather than drawing an empty box.
        let off = bands(&table(0, 0));
        assert!(off.bands.is_empty());
        let a = off.absence.expect("an empty table owes a line");
        assert!(a.contains("off"), "{a}");

        // A table that claims more than it carries says the read stopped short.
        let short = bands(&table(3, 1));
        assert_eq!(short.bands.len(), 1);
        assert!(
            short
                .caveat
                .expect("a short read owes a line")
                .contains("3"),
            "the caveat must name what the table claimed"
        );
    }

    /// ⚑ **An unseeded selector is refused, never decoded as an empty table.**
    ///
    /// `NOTE` §1: `BgAnim_Table_Ptr` = 0 is never valid. Decoding address 0 would read the 68000's vector
    /// table as bands and, worse, would usually report a zero count, which is the sentence for *bands are
    /// off* attached to a machine that has not initialised them.
    #[test]
    fn an_unseeded_band_selector_is_refused_rather_than_read_as_bands_being_off() {
        let mut f = Fake::full().serving(BANDS.selector, "0x00000000");
        let e = read_bands(&mut f).expect_err("a zero pointer is never valid");
        assert_eq!(e.reason.as_deref(), Some("selectorUnseeded"));
        assert!(e.message.contains("never valid"), "{}", e.message);

        // The control: a seeded pointer over a real table reads back.
        let mut table = 0u16.to_be_bytes().to_vec();
        table.resize(BAND_COUNT_BYTES + MAX_BANDS * BAND_RECORD_BYTES, 0);
        let hex: String = table.iter().map(|b| format!("{b:02X}")).collect();
        let mut f = Fake::full()
            .serving(BANDS.selector, "0x00028BD4")
            .serving("0x00028BD4", &format!("0x{hex}"));
        let b = read_bands(&mut f).expect("a seeded pointer");
        assert_eq!(b.count, 0);
    }

    /// ⚑ **A short read is refused, never zero-filled**, because a zero count word is the specific and
    /// wrong finding *bands are off* rather than a missing one.
    #[test]
    fn a_short_read_is_refused_rather_than_decoded_as_bands_being_off() {
        let mut f = Fake::full().serving(BANDS.selector, "0x0002");
        let e = read_bands(&mut f).expect_err("two bytes is not a longword");
        assert!(
            e.message
                .contains("would report bands as off rather than as unread"),
            "the refusal must say why a short read is not a zero: {}",
            e.message
        );
    }

    // ---------------------------------------------------------------------------------------------
    // ⚑ What is live, and the standing statement
    // ---------------------------------------------------------------------------------------------

    /// ⚑ **The live line is a READBACK and names what the cell points at**, never an echo of the click.
    ///
    /// The raster case is the one that matters and it is the second half: a selection stages
    /// `Raster_Pending`, `Raster_Program` does not move until the next `Raster_VBlank`, and on a paused
    /// machine this line must go on naming the OLD program. A panel that echoed the click would assert a
    /// swap that has not happened.
    #[test]
    fn the_live_line_reads_the_cell_back_and_does_not_echo_the_selection() {
        let mut f = Fake::full().serving(RASTER.selector, "0x0000881E");
        let l = live(&mut f, &RASTER).expect("a listed cell");
        assert_eq!(l.value, 0x0000_881E);
        assert_eq!(l.name.as_deref(), Some("Raster_Program_None"));
        let line = l.line(&RASTER);
        assert!(
            line.contains("Raster_Program") && line.contains("Raster_Program_None"),
            "{line}"
        );

        // Now select something else. The live cell has not moved, because only Raster_VBlank moves it.
        point_at(&mut f, &RASTER, "EditorRaster_OJZ_Act1_ramp_probe").expect("a listed program");
        let after = live(&mut f, &RASTER).expect("still readable");
        assert_eq!(
            after.name.as_deref(),
            Some("Raster_Program_None"),
            "the live line must report the cell, not the click: the swap happens at the next \
             Raster_VBlank and this machine has not run one"
        );

        // A value the listing does not name is said to be unnamed rather than dropped, and one that
        // lands past a symbol says so rather than claiming to be it.
        let past = Live {
            value: 0x0000_8820,
            name: Some("Raster_Program_None".into()),
            disp: 2,
        };
        assert!(
            past.line(&RASTER).contains("probably not"),
            "{}",
            past.line(&RASTER)
        );
    }

    /// ⚑ **A zero in a live cell reads as THAT CHANNEL's documented meaning**, and they differ.
    ///
    /// `NOTE` section 1 documents a zero raster program as *no program* with the engine short-circuiting
    /// on it, and a zero band pointer as *never valid* because `BgAnim_Init` seeds it. Rendering both as
    /// "the listing names nothing there" would report a documented off state as unreadable on one
    /// channel and an uninitialised machine as ordinary on the other.
    #[test]
    fn a_zero_live_cell_reads_as_that_channels_own_documented_meaning() {
        let zero = Live {
            value: 0,
            name: None,
            disp: 0,
        };
        let raster = zero.line(&RASTER);
        let band = zero.line(&BANDS);
        assert!(
            raster.contains("short-circuits") && !raster.contains("NOT A VALID"),
            "a zero raster program is a documented off state: {raster}"
        );
        assert!(
            band.contains("NOT A VALID STATE"),
            "a zero band pointer is a fault, not bands being off: {band}"
        );
        assert_ne!(
            raster, band,
            "the channels' zeroes are different findings and must not read alike"
        );
        // Loud on unmeasurable: every channel owes a sentence, so a channel added later cannot fall
        // through to a blank.
        for c in CHANNELS {
            assert!(
                zero.line(&c).len() > c.selector.len() + 16,
                "{} has no sentence for a zero live cell",
                c.key
            );
        }
    }

    /// ⚑ **A drifted selector silences the READBACK too, not only the write.**
    ///
    /// A panel that refuses to write a cell it cannot place, then prints a confident sentence about that
    /// same cell's contents, has answered the harder question and refused the easier one.
    #[test]
    fn a_drifted_selector_refuses_the_readback_as_well_as_the_write() {
        let mut f = Fake::full().serving(RASTER.selector, "0x0000881E");
        for e in f.listing.iter_mut() {
            if e.0 == RASTER.selector {
                e.2 = 0xFFFF_9000;
            }
        }
        let e = live(&mut f, &RASTER).expect_err("a moved cell has nothing honest to report");
        assert_eq!(e.reason.as_deref(), Some("selectorMoved"));

        // The control: undrifted, the same fake answers.
        let mut f = Fake::full().serving(RASTER.selector, "0x0000881E");
        assert!(live(&mut f, &RASTER).is_ok());
    }

    /// ⚑ **The standing statement exists exactly while an override does, and names channel and target.**
    ///
    /// The `None`-when-nothing half is as load-bearing as the other: a line standing over an untouched
    /// machine is a permanent false claim, which is the badge defect inverted.
    #[test]
    fn the_statement_stands_only_while_something_is_overridden_and_names_it() {
        assert_eq!(
            statement(&[]),
            None,
            "an untouched machine must claim nothing"
        );

        let mut changes = Vec::new();
        record(
            &mut changes,
            Change {
                key: "parallax".into(),
                title: "scene".into(),
                target: "ParallaxConfig_Haze".into(),
            },
        );
        let s = statement(&changes).expect("an override must say so");
        assert!(
            s.contains("scene") && s.contains("ParallaxConfig_Haze"),
            "it must name the channel and the target, not merely admit to a mode: {s}"
        );
        assert!(
            s.contains("not saved") || s.contains("Nothing here is saved"),
            "it must say the override is not written anywhere: {s}"
        );
        assert!(
            s.contains("section boundary"),
            "it must say how the override ends, because the answer is not this panel: {s}"
        );

        // A second change on the same channel REPLACES the first: a statement that grew would go on
        // naming a scene two selections ago, which is the staleness it exists to prevent.
        record(
            &mut changes,
            Change {
                key: "parallax".into(),
                title: "scene".into(),
                target: "ParallaxConfig_OJZ_Default".into(),
            },
        );
        assert_eq!(changes.len(), 1);
        let s = statement(&changes).unwrap();
        assert!(!s.contains("Haze"), "the old target must not survive: {s}");

        // A different channel stacks beside it rather than replacing it.
        record(
            &mut changes,
            Change {
                key: "raster".into(),
                title: "raster program".into(),
                target: "Raster_Program_None".into(),
            },
        );
        assert_eq!(changes.len(), 2);
        let s = statement(&changes).unwrap();
        assert!(
            s.contains("ParallaxConfig_OJZ_Default") && s.contains("Raster_Program_None"),
            "{s}"
        );
    }

    /// ⚑ **The nudge control's reason names the blocker and the two fields it will have.**
    ///
    /// The line is what stands in for a control that cannot work yet, so it has to answer the two
    /// questions a person asks: why is this off, and what will it do. A blank "coming soon" answers
    /// neither.
    #[test]
    fn the_disabled_nudge_control_says_why_and_what_it_will_be() {
        assert!(
            NUDGE_BLOCKED.contains("ROM"),
            "it must say WHY: the factors live in ROM and cannot be edited in place"
        );
        assert!(
            NUDGE_BLOCKED.contains("driver") && NUDGE_BLOCKED.contains("rate shift"),
            "NOTE section 2: only driver and rate_shift are meaningful, so the line must not promise \
             the whole record"
        );
        for barred in ["step mask", "col shift", "vram"] {
            assert!(
                !NUDGE_BLOCKED.to_lowercase().contains(barred),
                "the line promises {barred:?}, which NOTE section 2 rules out as geometry derived from \
                 the art"
            );
        }
    }

    /// **Every citation in this module points at a commit, never at a branch.**
    ///
    /// The rule this lane pointed back at the hub and had adopted: a tip moves and a commit does not.
    #[test]
    fn the_citations_name_a_commit_rather_than_a_tip() {
        for c in [NOTE, ENGINE] {
            assert!(
                c.contains("c4c5c3d8"),
                "{c:?} must cite the commit carrying the artifact"
            );
            for tip in ["master", "main", "HEAD", "origin/"] {
                assert!(!c.contains(tip), "{c:?} cites a moving tip: {tip}");
            }
        }
    }
}
