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
//! the destination from its own listing** — the same table `emulator/load_symbols` bound.
//!
//! ## ⚑ THE DISAGREEMENT IS A WITNESS, AND THE REFUSAL IS ON THE LAYOUT — THIS SECTION'S CORRECTION
//!
//! This module used to **refuse** the gesture whenever the resolved address and [`Channel::noted_addr`]
//! disagreed, on the argument that *"one of the two describes a different build and this panel cannot
//! tell which"*. **That symmetry is false and it cost two of the three channels every gesture.** The
//! resolved address comes from the listing of the ROM *actually loaded*; the note is a document written
//! earlier. For **access** the listing wins and there is nothing to adjudicate — and measured at this
//! seat on 2026-09-19 against `s4.debug.lst` (built 2026-09-18 19:26), two of three notes were already
//! stale, so the refusal was firing on every raster and every bands gesture on a **healthy** build:
//!
//! | channel | [`NOTE`] records | `s4.debug.lst` resolves |
//! |---|---|---|
//! | `Parallax_Current_Config` | `$FFFF88EC` | `$FFFF88EC` |
//! | `Raster_Program` | `$FFFF8BD6` | **`$FFFF8BF6`** |
//! | `BgAnim_Table_Ptr` | `$FFFFE91A` | **`$FFFFE93A`** |
//!
//! aeon moved them at `61918621` — the same sweep that moved the cursor — six symbols by `+$20` and one
//! by `+$200`. **An address is precisely the thing that slides harmlessly when unrelated RAM above it
//! grows, while the layout it was standing in for does not move at all**: not one width changed in that
//! sweep, and the parallax scratch's 542-byte span re-derives from `End − start` exactly as it did
//! before. So an address-equality test is a **proxy** for *is the layout still trustworthy*, and it is a
//! proxy that fails in the direction that refuses healthy builds. That is the same defect the cursor
//! guard was repaired for, in the section below, with the sign flipped.
//!
//! The real hazard the gate was reaching for is that the **struct shape** changed, which would make a
//! transcribed field offset address the wrong bytes. That is a layout question and it has a direct
//! instrument, the one [`hook`] already uses for the parallax scratch: **the derived span and the
//! published equates**. So this module now does three separate things where it used to do one:
//!
//! 1. **Access resolves.** Every read and every write addresses the cell by name and the server resolves
//!    it. Nothing consults [`Channel::noted_addr`] to decide where to write.
//! 2. **The note is kept as a WITNESS and the disagreement is STATED** ([`Channel::witness`],
//!    [`Drift`]). ⚑ This half is load-bearing and it is the half a later reader will be tempted to
//!    delete: a fix that simply drops `noted_addr` passes every test in this file and destroys the only
//!    thing that would catch the next sweep. Keeping the number and *saying* it disagrees is what makes
//!    drift **visible instead of silent**. [`SCRATCH_NOTED_ADDR`] is the same pattern and is named for
//!    it.
//! 3. **The refusal is on a layout fact** ([`layout`]), never on an address: each cell's bytes must
//!    still lie inside the symbol the cell names, and an array a write-set poisons whole must still be
//!    exactly as large as the write-set poisons and must still partition by the count the listing
//!    publishes. Both are derived from the **loaded listing** per gesture. Add `+$20` to every symbol in
//!    the game and not one of those facts changes.
//!
//! # ⚑ `Debug_Lab_Index` IS NEVER WRITTEN, AND IT IS GUARDED TWICE
//!
//! [`LAB_INDEX_SYMBOL`] is the START chord's **cursor**. Writing it moves the label on screen and changes
//! nothing that runs, which is a display that lies — the worst outcome available to a panel whose job is
//! telling a person what the machine is doing. [`NOTE`] prices it: *"This cost this lane an hour today —
//! the label said one row while the machine ran another."*
//!
//! [`forbidden`] refuses it **by name and by address independently**, because those are two different
//! routes in: a write-set edited to name it, and a cell whose symbol happens to resolve there. Neither
//! guard subsumes the other and both are gated.
//!
//! ⚑ **The address route is keyed on what the LOADED LISTING resolves the name to** ([`Cursor`]), never on
//! a number written down here, and that is this section's one hard-won rule rather than a style choice.
//! It was a transcribed `$FFFFEE0D` until aeon swept its own RAM table (aeon `61918621`) and found the
//! cursor had moved to `$FFFFF00D` — `+$200`, while six sibling symbols slid `+$20`, so not even
//! recoverable by applying the others' offset. A stale number breaks the guard in **both** directions at
//! once: the real cursor stops being refused, *and* the old address — `$FFFFEE0D` is now 13 bytes into
//! `Player_Pos_Ring` (`$FFFFEE00`) in `s4.debug.lst` — starts being refused under the cursor's name, which
//! is a refusal that misidentifies what it caught. aeon's own remedy for the class is the one adopted
//! here: **resolve, do not transcribe**, because updating the number only rewinds a clock nobody winds.
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
//! selected. `nothing_the_effects_panel_selects_can_reach_the_saved_layout` in [`crate::layout`] is the
//! gate.

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

/// The equate the listing publishes for how many band indices `BgAnim_LastStep` has a slot for.
///
/// ⚑ **The name is the only thing transcribed here; the value is resolved per gesture.** [`MAX_BANDS`]
/// carries `4` as a literal from [`NOTE`] and is used for *decoding a table read out of ROM*, which this
/// equate cannot replace — see that constant. This one is used for one thing: [`layout`]'s coverage
/// gate on the poison array. Measured 2026-09-19: `s4.debug.lst` publishes `BGANIM_MAX_BANDS = $4`.
pub const BGANIM_MAX_BANDS_EQU: &str = "BGANIM_MAX_BANDS";

// ⚑ There is deliberately NO `LAB_INDEX_ADDR` constant here. [`forbidden`]'s address route is keyed on
// [`Cursor`], which resolves [`LAB_INDEX_SYMBOL`] out of the loaded listing per gesture. See the module
// header: the constant this file used to carry went stale and made the guard wrong in both directions.
// The note's number survives in exactly one place — `tests::NOTED_STALE_LAB_INDEX`, whose whole job is to
// be the value the guard must NOT be keyed on — and nothing in the shipped path reads it.

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

/// ⚑ **A symbol whose ENTIRE extent a write-set poisons**, and the equate that counts its elements.
///
/// This is the second half of [`layout`] and it exists because the per-cell check below cannot see an
/// array that **grew**. `BgAnim_LastStep` is the only one in this module: the write-set poisons eight
/// bytes as two longwords, and *"the state array is per band index, not per band identity"*
/// (`bg_anim.emp`'s own header), so a build that gave the array a fifth band would leave that band's
/// stale step in place and *"the new table would never paint"* on it — silently, on a write-set whose
/// every cell still landed inside its symbol.
///
/// The two facts it pins are both the loaded listing's:
///
/// * **The extent equals what the write-set covers**, pinned with two probes rather than believed: no
///   symbol starts *inside* the covered bytes, and one starts *exactly* one past them.
/// * **That extent partitions by [`Covering::count_equate`]**, so the bytes-per-element the poison
///   assumes is derived rather than transcribed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Covering {
    /// The array symbol. Every cell of the write-set naming it counts toward the coverage.
    pub symbol: &'static str,
    /// The equate the loaded listing publishes for the element count. **Never a number here.**
    pub count_equate: &'static str,
    /// What one element is, for the sentence. `"band index"`.
    pub element: &'static str,
    /// The line of [`ENGINE`] that says the whole array must be poisoned, and what goes wrong otherwise.
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
    /// for [`Channel::witness`] and nothing else. Never written to, never sent, and **never compared
    /// against in order to refuse** — see the module header's correction: two of these three were
    /// already stale on 2026-09-19 and refusing on them refused a healthy build.
    ///
    /// ⚑ **These are deliberately NOT re-transcribed to what the listing says today.** Updating the
    /// number only rewinds a clock nobody winds, and a witness that agrees with the listing witnesses
    /// nothing. The number is the note's, dated by [`NOTE`], and the *gap* is the finding.
    pub noted_addr: u32,
    /// **What a selection writes**, in order, transcribed from the engine's own installer.
    pub writes: &'static [Cell],
    /// ⚑ **Arrays this write-set poisons WHOLE**, for [`layout`]'s second gate. Empty on a channel whose
    /// write-set writes only scalars — which is two of the three, and that emptiness is a measured
    /// finding rather than an omission: see each channel's own note.
    pub covers: &'static [Covering],
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
    // ⚑ **Measured, not skipped.** Every cell above is `disp: 0` on a symbol of its own, so this
    // write-set carries **no transcribed field offset at all** and there is no array in it to poison
    // whole. [`layout`]'s per-cell check is the whole of the layout fact this channel depends on. (The
    // parallax *scratch* surface does carry offsets, and they are resolved from equates already —
    // [`hook`], which is where the instrument this gate borrows came from.)
    covers: &[],
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
    // ⚑ **Measured, not skipped.** One cell, `disp: 0`, one longword into a longword cell: no
    // transcribed offset and no array. [`layout`]'s per-cell check is the whole of it here.
    covers: &[],
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
    // ⚑ **THE ONE TRANSCRIBED LAYOUT FACT IN ANY OF THE THREE WRITE-SETS**, and therefore the one place
    // a layout gate has something of its own to test. The `disp: 4` above says *this array is eight
    // bytes*, and `bg_anim.emp:185-186` is where that came from. Both halves of the claim are checked
    // against the loaded listing per gesture: `BgAnim_LastStep`'s extent must be exactly the eight bytes
    // poisoned, and it must divide by the count the listing publishes.
    covers: &[Covering {
        symbol: "BgAnim_LastStep",
        count_equate: BGANIM_MAX_BANDS_EQU,
        element: "band index",
        why: "BgAnim_SetTable poisons the WHOLE array (bg_anim.emp:185-186) because the state is per \
              band INDEX, not per band identity. A band left un-poisoned whose step matches the \
              outgoing table's takes the .skip_band arm forever and the new table never paints on it",
    }],
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
    /// `Some` is a **witness**, never a refusal, and that is this parcel's correction. The old shape
    /// refused, on the argument that the panel could not tell a stale note from a mismatched listing.
    /// **The two are not symmetric.** `resolved` came from the listing of the ROM actually loaded —
    /// the one `emulator/load_symbols` bound and reported a `binding` for — and [`NOTE`] is a document
    /// written earlier. For deciding *where to write*, the listing wins outright and there is nothing to
    /// adjudicate; the panel never held a choice here, only a veto it was exercising against healthy
    /// builds. (The mismatched-listing hazard is real and is `load_symbols`' `binding` to report; a
    /// stale transcription in this crate is not the instrument for it, and cannot be, because it is
    /// equally consistent with an ordinary aeon commit.)
    ///
    /// What survives is the **statement**: both numbers, named, wherever the panel reports on this
    /// channel. That is what makes the next sweep visible instead of silent, and it is the half of this
    /// change most likely to be dropped by the next person who finds `noted_addr` unused-looking.
    ///
    /// ⚑ **`resolved` is the listing's 32-bit `rawAddr`**, which is the spelling
    /// [`Channel::noted_addr`] is transcribed in (`$FFFF88EC`, not `$FF88EC`). Comparing the 24-bit door
    /// form would make every channel read as drifted, which would now be a wrong *sentence* rather than
    /// a wrong refusal — still wrong, and still the same class of mistake as the dead guard in
    /// [`forbidden`].
    pub fn witness(&self, resolved: u32) -> Option<Drift> {
        (resolved != self.noted_addr).then_some(Drift {
            selector: self.selector,
            noted: self.noted_addr,
            resolved,
            now: None,
        })
    }
}

/// ⚑ **The note and the listing disagree about where a selector is, stated rather than enforced.**
///
/// See [`Channel::witness`]. This type is the witness's whole visible form: it is drawn beside the live
/// readback and appended to a gesture's readout, and nothing branches on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Drift {
    /// [`Channel::selector`].
    pub selector: &'static str,
    /// [`Channel::noted_addr`] — what [`NOTE`] recorded.
    pub noted: u32,
    /// What the **loaded listing** resolves the same name to, and what every access uses.
    pub resolved: u32,
    /// ⚑ **What the note's old address is now**, resolved out of the same listing: the nearest preceding
    /// label and how far past it. `None` when nothing filled it in — see [`witnessed`].
    ///
    /// This is the sentence's sharpest half and it is the lesson the cursor guard was repaired for,
    /// restated: a stale address does not go vacant, it becomes **somebody else's cell**. `$FFFF8BD6`
    /// is `Region_Cur_X0` in `s4.debug.lst` as built 2026-09-18, not a hole where `Raster_Program`
    /// used to be.
    pub now: Option<String>,
}

impl Drift {
    /// The witness sentence, as the panel draws it.
    ///
    /// It states the gap and then states which side is authoritative, because a reader who is told only
    /// that two numbers differ has been handed the panel's old confusion rather than its answer.
    pub fn line(&self) -> String {
        let now = match &self.now {
            Some(what) => format!(" {:#010X} is now {what}.", self.noted),
            None => String::new(),
        };
        format!(
            "DRIFT (stated, not blocking): the loaded listing puts `{}` at {:#010X} and {NOTE} records \
             {:#010X}.{now} Every read and write on this channel used the LISTING's address, which is \
             the build actually loaded; the note's number is kept only so this gap is visible. If the \
             listing is not the one this ROM was built with, the thing that reports it is \
             `emulator/load_symbols` and its `binding`, and nothing here can.",
            self.selector, self.resolved, self.noted
        )
    }
}

/// **[`Channel::witness`] with [`Drift::now`] filled in from the loaded listing.**
///
/// One extra `emulator/lookup_symbol` and **only on the drifted path**, so an agreeing channel costs
/// nothing. A lookup that fails is an answer (`now` stays `None`) rather than a failure: the witness is
/// a statement and must not be able to turn into a refusal by a side road.
fn witnessed(c: &mut impl Caller, channel: &Channel, resolved: u32) -> Option<Drift> {
    let mut d = channel.witness(resolved)?;
    if let Ok(v) = c.call(
        "emulator/lookup_symbol",
        serde_json::json!({ "addr": format!("0x{:06X}", d.noted & 0x00FF_FFFF) }),
    ) {
        if let Some(name) = v["name"].as_str() {
            d.now = Some(match v["disp"].as_u64().unwrap_or(0) {
                0 => format!("`{name}`"),
                n => format!("${n:X} past `{name}`"),
            });
        }
    }
    Some(d)
}

/// ⚑ **Where the START chord's cursor actually is**, resolved out of the loaded listing, or the stated
/// reason [`forbidden`]'s address route has nothing to key on.
///
/// This type exists so that the address route cannot be a transcribed number again. It is the shape aeon's
/// remedy prescribes — *resolve, do not transcribe* — and it is the opposite of [`SCRATCH_NOTED_ADDR`],
/// which is a literal kept **deliberately** as a witness to what a note said. A witness may be a literal;
/// a claim about the running machine may not.
///
/// Resolved **per gesture and never cached**, the same rule §11.26 imposes on every other name this module
/// reads: a listing can be swapped between one gesture and the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    /// The 32-bit `rawAddr` the loaded listing gives [`LAB_INDEX_SYMBOL`], or `None` when **this listing**
    /// does not carry the name.
    ///
    /// ⚑ `None` is *did not resolve in this listing*, never *does not exist*: it is the ordinary answer for
    /// a release listing and for a game with no lab at all (measured 2026-09-19 — `s4.debug.lst` carries
    /// the name, and `s4.lst`, `demo.lst` and `demo.debug.lst` do not), and it is equally the answer for a
    /// listing that merely predates the symbol.
    pub raw: Option<u32>,
}

impl Cursor {
    /// **Which of the two routes is live**, as a clause [`forbidden`] puts inside every refusal it raises.
    ///
    /// It is a clause on the refusal rather than a standing caveat on the panel, and that is a decision
    /// with a reason: on three of the four listings on this box the name does not resolve, so a banner
    /// saying so would be drawn on nearly every gesture — the **unconditional caveat** this tree forbids in
    /// terms (`oracle-core/src/render.rs`, `oracle-frontend/src/pick.rs` §11.27). Attached to the refusal
    /// it is conditional on the guard actually speaking, and it is exactly then that a reader needs to know
    /// how much of the guard ran.
    fn routes(&self) -> String {
        match self.raw {
            Some(raw) => format!(
                "the loaded listing puts it at {raw:#010X}, so BOTH routes are live: the name and that \
                 address"
            ),
            None => format!(
                "⚠ ONLY THE NAME ROUTE RAN. `{LAB_INDEX_SYMBOL}` is not a name in the loaded listing, so \
                 the address route had no address to key on and was not run, so a cell resolving onto the \
                 cursor under some other name would NOT have been caught. That is the ordinary shape of a \
                 release listing and of a game with no lab, and it is equally what an out-of-date listing \
                 looks like; this panel cannot tell those apart and does not guess"
            ),
        }
    }
}

/// **Resolve the cursor out of the loaded listing**, for [`forbidden`] to key its address route on.
///
/// ⚑ **A listing that does not carry the name does NOT refuse the feature**, and that choice is the whole
/// of this function. The alternative — treating an unresolvable cursor as a precondition failure — would
/// make a panel that selects parallax scenes go dark because of a symbol it never writes, and on a release
/// listing it would go dark *always*. This lane made the same call for [`SCRATCH_NOTED_ADDR`] and for the
/// same reason: a gate whose red is about the layout rather than about the subject teaches nothing.
///
/// So the two outcomes are separated rather than collapsed:
///
/// * the bus answered and this listing has no such name (`notInListing`) → `Cursor { raw: None }`, the
///   name route still refuses, and [`Cursor::routes`] says in the refusal that the address route did not
///   run. Honest, and not silent;
/// * anything else — no listing loaded at all, a malformed reply, a dead bus — is **propagated**. Those
///   are not statements about this symbol, and swallowing them here would let a broken bus read as a build
///   without a lab.
pub fn cursor(c: &mut impl Caller) -> Result<Cursor, Refusal> {
    match resolve(c, LAB_INDEX_SYMBOL) {
        Ok((_, raw)) => Ok(Cursor { raw: Some(raw) }),
        Err(e) if e.reason.as_deref() == Some("notInListing") => Ok(Cursor { raw: None }),
        Err(e) => Err(e),
    }
}

/// ⚑ **The guard that keeps [`LAB_INDEX_SYMBOL`] unwritable**, by name and by address independently.
///
/// Two checks rather than one, because they catch two different mistakes and neither subsumes the other:
/// a write-set edited to name the cursor, and a cell whose symbol *resolves* to the cursor's address in
/// some build. A panel that only checked the name would happily poke the cursor's cell through a symbol
/// called something else; one that only checked the address would miss it the day the cursor moves.
///
/// ⚑ **`cursor` is the listing's answer, not a constant** — see [`Cursor`] and the module header. The
/// address route fires only when the name resolved; when it did not, the refusal says so rather than the
/// panel quietly checking half of what it advertises.
///
/// ⚑ **`raw_addr` is the listing's 32-bit spelling (`rawAddr`), never the 24-bit form the memory doors
/// take**, because that is the spelling [`resolve`] reports the cursor in too, and the comparison has to be
/// like for like. Handing this the door form makes the address branch **dead** — `$FFF00D` never equals
/// `$FFFFF00D` — so the guard would pass every real input. That is not hypothetical: it is what this
/// function was doing until `the_address_route_fires_on_a_resolved_symbol_and_not_only_on_the_constant` was
/// written, and the row exists so it cannot come back.
///
/// `None` means the write may proceed. Deliberately not a `bool`: the caller must have a sentence.
pub fn forbidden(cursor: Cursor, symbol: &str, raw_addr: u32) -> Option<Refusal> {
    let how = if symbol == LAB_INDEX_SYMBOL {
        "it is named as the write target"
    } else if cursor.raw == Some(raw_addr) {
        "it resolves to that address"
    } else {
        return None;
    };
    Some(Refusal::window(
        "labIndexIsNotASelector",
        format!(
            "refused: `{LAB_INDEX_SYMBOL}` is the START chord's cursor, not a selector, and {how}. \
             Writing it moves the label on screen and changes nothing that runs, which is a display that \
             lies about what the machine is doing. {NOTE} prices that mistake at an hour of aeon's day: \
             the label said one row while the machine ran another. {}",
            cursor.routes()
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
// ⚑ THE NUDGE CONTROLS, WHICH NOW SHIP — the RAM scratch config, and the two conditions that gate it
// -------------------------------------------------------------------------------------------------------

//
// ⚑ **THE BLOCKER IS GONE, AND THE FIRST THING TO SAY IS WHICH BLOCKER.** `NOTE` §4 and the switchboard
// design's §5.2 both said numeric nudging was *"GENUINELY BLOCKED"* and the hub ruled *"nudge controls do
// not ship until it lands."* It landed, at [`HOOK`]: a RAM scratch parallax config, an arm cell, and
// `Parallax_InstallScratch`, which copies the active ROM config into the scratch and re-points the
// selector at it. The panel edits the RAM copy and the next frame picks it up. So a factor is editable in
// place after all — in a copy, which is the same thing from a knob's point of view.
//
// # ⚑ IT IS THE PARALLAX CONFIG, NOT THE BGANIM BAND RECORD, AND THAT CORRECTS §5.2's OWN SENTENCE
//
// §5.2 promised *"two numbers, `driver` and `rate_shift`"* when the hook arrived, citing `NOTE` §2. **Those
// two fields are not in this hook and cannot be reached by it.** They are fields of the **BgAnim band
// record** ([`BAND_RECORD_BYTES`], the table `BgAnim_Table_Ptr` selects), and the hook that landed is the
// **parallax** channel's: its buffer holds a `parallax_config` — a header plus band records whose fields
// are scroll-factor shifts, not drivers. `NOTE` §6.5 says so in its own words, having noticed the same
// thing from the other side: *"The parallel with §2's band-record advice holds, and the answer is **not**
// the same one: there, `driver`/`rate_shift` were nudgeable and `step_mask`/`col_shift` were art geometry.
// Here the division is **three-way**."*
//
// So §5.2's *"two numbers"* was a forecast made about one channel's fields from the other channel's note,
// and it did not survive the hook actually landing. What this module offers instead is stated in
// [`FIELDS`] and what it refuses is stated in [`NOT_OFFERED`], with `driver` and `rate_shift` among the
// refusals — carrying the reason they are refused, which is **not** that they are geometry (they are not)
// but that **an act's `BgAnim_Table` is ROM and no RAM copy of it exists in any shape**. That is an
// engine-side gap, named here rather than papered over, exactly as `NOTE` §3 named the bands-off gap.
//
// # ⚑ THE TWO CONDITIONS, AND THE TRAP THEY EXIST FOR
//
// The precedent is the switchboard design's §5.3, and its lesson is one sentence: **a name resolving is
// not storage existing.** For bands-off, `BgAnim_Table_Empty`'s NAME enters a release listing with an
// address while its array emits nothing behind it, so a gate keyed on that one symbol would have written a
// real address into a cell that was not the destination.
//
// **This hook has the identical shape and `NOTE` §6.6 measured it**: `Parallax_InstallScratch` appears in
// the RELEASE listing with an address — its body is inside `if DEBUG == 1`, so the label collapses onto its
// neighbour's — while `Parallax_Scratch_Config` and `Parallax_Scratch_Arm` do **not**. A panel that decided
// "the hook is available" by resolving the proc would offer knobs on a release build and then write into
// whatever occupies the scratch's old address, with no fault to show for it.
//
// So [`hook`] gates on **two** conditions and resolves the **destination first**:
//
// 1. **[`SCRATCH`] resolves** — the buffer this panel writes into. Genuinely absent from a release
//    listing, because `engine/ram.emp:1811` declares it inside `if DEBUG == 1 @shape_divergent`.
// 2. **[`SCRATCH_ARM`] resolves** — the request cell the install is asked for through. Same block, same
//    shape gate; a build could in principle carry one and not the other, and a panel that armed a cell
//    with no buffer behind it would report an install that copied into nothing.
//
// **[`SCRATCH_PROC`] is deliberately never consulted**, and
// `the_gate_ignores_the_proc_because_its_name_ships_in_a_release_listing` is the row that keeps it that
// way.
//
// # ⚑ THE OFFSETS COME OUT OF THE LISTING, NOT OUT OF THIS FILE
//
// `NOTE` §6.3 and §6.4 tabulate every field's offset, and transcribing them would have been the obvious
// thing. **Two measurements say not to**, both made at this seat against the listings on this box:
//
// * **The scratch has already MOVED.** `NOTE` §6.1 records `Parallax_Scratch_Config` at `$FFFFEA26`;
//   `s4.debug.lst` as built on 2026-09-18 puts it at **`$FFFFEA46`**. It is at the RAM tail inside a
//   `@shape_divergent` group, so it moves whenever any other debug-RAM group changes size — which is an
//   ordinary aeon commit, not a mistake. A positional refusal on this symbol would refuse the whole
//   feature on a healthy build, so the noted addresses are kept as **witnesses only**
//   ([`SCRATCH_NOTED_ADDR`]) and nothing refuses on them. ⚑ **That finding has since been generalised**:
//   [`Channel::witness`] is the same decision for the three channel selectors, which used to refuse on
//   the same comparison and were refusing two of three gestures on a healthy build by the time it was
//   measured. This surface got there first.
// * **The band-record stride is PER GAME.** It is 32 bytes in `s4.debug` and **10** in `demo.debug`
//   (measured: span `$FFFFE60E - $FFFFE550` = 190 = 30 + 10 × 16). `NOTE` §6.4's *"`sizeof(band_record)`
//   is **32** for this game"* says so, and a 32 transcribed here would have addressed demo's band 1 inside
//   its band 3.
//
// What the listing publishes instead is **the struct layout itself**, as equates: `parallax_config_len`,
// `parallax_config_pcfg_layer_mask`, `band_entry_band_factor_a_s1`, `MAX_PARALLAX_BANDS`. Those reach a
// client through `emulator/lookup_equate` (§11.36, already served and already vendored — no contract change
// was needed for this parcel). So every offset this module writes at is **resolved per gesture from the
// listing the machine is running with**, exactly as every address already is, and the stride is *derived*
// from two symbols and one equate rather than believed:
//
// ```text
// span   = Parallax_Scratch_Config_End - Parallax_Scratch_Config
// stride = (span - parallax_config_len) / MAX_PARALLAX_BANDS
// ```
//
// which is the same arithmetic `engine/ram.emp` sizes the buffer with, run backwards. It yields 32 on
// `s4.debug` and 10 on `demo.debug`, and a build that widens a band record cannot leave this module
// addressing the old stride. **A non-exact division is a refusal, never a rounded stride**: it means the
// premise (one header, `MAX_PARALLAX_BANDS` equal-sized records, nothing else in the span) does not hold
// for this build, and a rounded stride would write into the middle of fields forever after.
//
// # ⚑ EDITING A SCRATCH THAT IS NOT THE CURRENT CONFIG IS THE SILENT NO-OP THIS WHOLE SURFACE EXISTS AGAINST
//
// The scratch is ordinary work RAM. Writing a byte into it always succeeds and means **nothing** unless
// `Parallax_Current_Config` points at it. Two ordinary events leave it that way: nobody has armed yet, and
// `NOTE` §6.6's consequence 1 — **crossing a section boundary EVICTS the scratch**, because
// `Parallax_CheckBoundary` installs the new section's own ROM preset exactly as it always did.
//
// So [`nudge`] **re-checks the install before every write** and refuses when it does not hold. That check
// is not a nicety: without it the panel would let a person turn a knob and watch nothing happen after
// walking across a boundary, which is precisely the failure `NOTE` §0 was written about.
//
// # ⚑ "DID THE INSTALL TAKE" IS ANSWERED BY THE FACT, NOT BY A STATUS BYTE
//
// `NOTE` §6.2 is explicit that the arm cell is a **request** byte and not a status byte — the engine clears
// it as it services it, whether the install took or was refused — and that the success test is reading
// `Parallax_Current_Config` and comparing it against `Parallax_Scratch_Config`. That is *"the fact itself
// rather than a report of it"*, and [`Installed`] is that comparison and nothing else.
//
// The arm byte is read back too, and **only to separate two failures that the comparison alone renders
// identical**:
//
// | `Current_Config` | arm byte | what it is |
// |---|---|---|
// | == the scratch | (either) | **installed** |
// | != the scratch | cleared | the engine **serviced and REFUSED** it: no active config, or the config's `pcfg_band_count` exceeds `MAX_PARALLAX_BANDS`. `Parallax_InstallScratch`'s own `Out:` — *"nothing written … a refusal never clamps"* |
// | != the scratch | still set | the arm was **never serviced**: `Parallax_Update` did not reach its poll this frame. `NOTE` §6.6's banner names this exactly for `games/demo`, where `Parallax_Update` has no caller at all — *"arming on demo leaves the arm cell SET for ever, which reads like a dirty refusal and is nothing of the kind"* |
//
// That is two facts read off the machine yielding three states. No byte was invented and no byte is
// interpreted as a status.
//
// ⚑ **The comparison is masked to 24 bits on BOTH sides.** `Parallax_Current_Config` holds the full
// sign-extended long (`$FFFFEA46`) and a listing may resolve the symbol either way; `NOTE` §6.2 warns that
// *"a raw compare is a false mismatch, and it was the first thing this lane's own probe got wrong."* The
// space of values is the whole question here rather than the pair that happened to be measured: masking
// both sides is correct for all four combinations of spelling, and comparing raw is correct for one.
//

/// **The commit that landed the parallax scratch hook**, cited separately from [`NOTE`] because it
/// postdates the note's own §4 and inverts it: §4 said *"until it lands, a nudge control has nothing to
/// write and should not ship"*, and this is it landing.
///
/// Read firsthand out of aeon's engine rather than out of the doc, and for a stated reason: the note's §4
/// header says *"LANDED"* in one sentence and *"are on branch `parcel/live-effects-hook`"* in the next,
/// which are different claims. `engine/ram.emp:1811` and `engine/level/parallax.emp:4208` settle it.
pub const HOOK: &str = "aeon 935c33cf";

/// The RAM working copy a nudge writes into. **DEBUG shapes only** — `engine/ram.emp:1811` declares it
/// inside `if DEBUG == 1 @shape_divergent`, so a release build emits zero bytes for it and its name is
/// absent from a release listing. Condition 1 of [`hook`]'s gate, and resolved **first**.
pub const SCRATCH: &str = "Parallax_Scratch_Config";

/// The `mark` one past the scratch's end. Its distance from [`SCRATCH`] is the buffer's size, which is
/// where [`Hook::stride`] comes from.
pub const SCRATCH_END: &str = "Parallax_Scratch_Config_End";

/// The request cell. Nonzero asks `Parallax_Update`'s head poll to install the scratch; the engine clears
/// it as it services it. **DEBUG shapes only**, same block. Condition 2 of [`hook`]'s gate.
pub const SCRATCH_ARM: &str = "Parallax_Scratch_Arm";

/// ⚑ **The proc, named here ONLY so the reason it is not the gate has somewhere to live.**
///
/// `NOTE` §6.6 measured it: this name **appears in the RELEASE listing with an address** while [`SCRATCH`]
/// and [`SCRATCH_ARM`] do not, because the proc's body — the `rts` included — is inside `if DEBUG == 1`
/// and an empty label collapses onto its neighbour's address. Gating on it would offer knobs on a release
/// build. Nothing in this module resolves it.
pub const SCRATCH_PROC: &str = "Parallax_InstallScratch";

/// [`SCRATCH`]'s address as [`NOTE`] §6.1 records it, **kept as a witness and never compared against**.
///
/// It is measured: the note says `$FFFFEA26` and `s4.debug.lst` built 2026-09-18 says `$FFFFEA46`. The
/// symbol is at the RAM tail inside a size-varying `@shape_divergent` group, so it moves on ordinary
/// aeon commits, and a refusal here would refuse a healthy build.
///
/// ⚑ **This used to read that `Parallax_Current_Config` is engine RAM at a fixed offset and is a
/// different case, which is why it was still drift-checked. That sentence was wrong**, and it is
/// corrected here rather than deleted because it is exactly the reasoning the channel refusal rested
/// on. Engine RAM at a fixed offset inside its own block still moves whenever a block ABOVE it changes
/// size, which is what aeon `61918621` did: `Raster_Program` slid `$20` and `BgAnim_Table_Ptr` slid
/// `$20` while nothing about their layout changed at all. There is no "different case"; there is one
/// case, and [`Channel::noted_addr`] is now the same kind of witness this constant always was.
pub const SCRATCH_NOTED_ADDR: u32 = 0xFFFF_EA26;

/// The equate giving `sizeof(parallax_config)` — the header's length, and band record 0's offset.
pub const CONFIG_LEN_EQU: &str = "parallax_config_len";

/// The equate giving the number of band records the scratch is reserved for. `16` in both shipped games.
pub const MAX_BANDS_EQU: &str = "MAX_PARALLAX_BANDS";

/// The equate prefix the `parallax_config` header's field offsets are published under.
pub const HEADER_EQU_PREFIX: &str = "parallax_config_";

/// The equate prefix a band record's field offsets are published under.
///
/// ⚑ **`band_entry`, not `band_record`.** The struct the scratch strides by is `band_record` — the legacy
/// `band_entry` plus this game's capability tails — and **only the `band_entry` half publishes equates**
/// (measured: `s4.debug.lst` carries nine `band_entry_*` rows and `band_entry_len`, and no `band_record_*`
/// row at all). Every field this module offers is inside that half, which is why the offsets resolve; the
/// tails' fields are in [`NOT_OFFERED`] for other reasons anyway, so nothing is lost. [`Hook::stride`] is
/// derived from the span rather than from `band_entry_len`, precisely because the two differ.
pub const BAND_EQU_PREFIX: &str = "band_entry_";

/// Which half of the buffer a [`Field`] lives in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Where {
    /// The `parallax_config` header, once. Offset is the equate's value.
    Header,
    /// One band record. Offset is `header_len + stride * index + equate`.
    Band,
}

/// **One field a person may turn**, with the equate its offset is resolved from and the range that is
/// coherent rather than merely accepted.
///
/// There is no offset in here on purpose. See this section's header: the offsets are the listing's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    /// The stable key a gesture names this field by. Never drawn.
    pub key: &'static str,
    /// What a person reads on the control.
    pub label: &'static str,
    /// The equate whose value is this field's offset, **without** the prefix.
    pub equate: &'static str,
    /// [`Where`] the field lives.
    pub at: Where,
    /// 1 or 2, as the struct declares it and as the door takes it.
    pub width: u8,
    /// The inclusive range that is coherent. **Not** the range the door accepts: the door accepts every
    /// value a `u8` can hold, and `NOTE` §6.4 says which of them mean something.
    pub range: (u32, u32),
    /// What turning it does, and what the ends mean. Drawn as the control's hover.
    pub what: &'static str,
}

impl Field {
    /// The equate name this field's offset resolves from, prefix included.
    pub fn equate_name(&self) -> String {
        let prefix = match self.at {
            Where::Header => HEADER_EQU_PREFIX,
            Where::Band => BAND_EQU_PREFIX,
        };
        format!("{prefix}{}", self.equate)
    }
}

/// ⚑ **THE FIELDS THIS PANEL OFFERS**, and every one of them is in `NOTE` §6.5's *"free knobs"* class:
/// nothing else in the config depends on it, so turning it produces a picture whose parts still agree.
///
/// The three-way division §6.5 draws is the whole reason this list is short. An *inert* field is a slider
/// that does nothing — the defect `NOTE` §0 exists to prevent — and a *coupled* field produces a picture
/// whose parts disagree, which is a different and subtler wrong. Both classes are in [`NOT_OFFERED`] with
/// their reasons, because a reader who wants `band_top_plane` deserves to find out why it is absent here
/// rather than conclude it was forgotten.
pub const FIELDS: &[Field] = &[
    Field {
        key: "layer_mask",
        label: "layer mask",
        equate: "pcfg_layer_mask",
        at: Where::Header,
        width: 2,
        range: (0x0000, 0xFFFF),
        what: "bit i = band i active. Clearing a bit drops that band and it inherits the band above. \
               NOTE 6.3 calls this the best knob here: free, instant, reversible",
    },
    Field {
        key: "deform_speed_fg",
        label: "FG deform speed",
        equate: "pcfg_deform_speed_fg",
        at: Where::Header,
        width: 1,
        range: (0, 255),
        what: "Plane A horizontal-deform phase increment per frame. 1 is what a scene with no table emits",
    },
    Field {
        key: "deform_speed_bg",
        label: "BG deform speed",
        equate: "pcfg_deform_speed_bg",
        at: Where::Header,
        width: 1,
        range: (0, 255),
        what: "the same for Plane B",
    },
    Field {
        key: "bob",
        label: "bob",
        equate: "pcfg_bob",
        at: Where::Header,
        width: 1,
        range: (0, 255),
        what: "packed, and the WHOLE BYTE 0 means no bob: the sentinel is the byte, not a nibble. \
               Otherwise bits 7-4 are the amplitude shift (legal 1..8) and bits 3-0 the period shift \
               (legal 0..8), so a nonzero byte outside those nibble ranges is a sway the engine will \
               still draw and nobody authored",
    },
    Field {
        key: "factor_a_s1",
        label: "A shift 1",
        equate: "band_factor_a_s1",
        at: Where::Band,
        width: 1,
        range: (0, 15),
        what: "Plane A scroll shift 1, NOTE 6.4's main knob. 0..14 is a shift; 15 means whole-factor \
               zero, the band locked to the camera",
    },
    Field {
        key: "factor_a_s2",
        label: "A shift 2",
        equate: "band_factor_a_s2",
        at: Where::Band,
        width: 1,
        range: (0, 15),
        what: "Plane A scroll shift 2. 0..14 is a shift; 15 means single-term, use shift 1 alone",
    },
    Field {
        key: "factor_b_s1",
        label: "B shift 1",
        equate: "band_factor_b_s1",
        at: Where::Band,
        width: 1,
        range: (0, 15),
        what: "Plane B scroll shift 1, same sentinels",
    },
    Field {
        key: "factor_b_s2",
        label: "B shift 2",
        equate: "band_factor_b_s2",
        at: Where::Band,
        width: 1,
        range: (0, 15),
        what: "Plane B scroll shift 2, same sentinels",
    },
    Field {
        key: "factor_ops",
        label: "ops",
        equate: "band_factor_ops",
        at: Where::Band,
        width: 1,
        range: (0, 3),
        what: "bit 0: Plane A adds (0) or subtracts (1) the second term. bit 1: Plane B, the same. \
               Bits 2-7 are unread",
    },
    Field {
        key: "phase_offset",
        label: "phase",
        equate: "band_phase_offset",
        at: Where::Band,
        width: 1,
        range: (0, 255),
        what: "added to this band's deform sample index, to desync it from its neighbours",
    },
];

/// **One field this panel does NOT offer, and the reason**, so an absence is never mistaken for an
/// oversight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotOffered {
    /// The field, as `NOTE` and the engine spell it.
    pub field: &'static str,
    /// Which of the reasons it is. Drawn as a short tag before [`NotOffered::why`].
    pub class: &'static str,
    /// Why, in the words a person can act on.
    pub why: &'static str,
}

/// ⚑ **THE REFUSALS, WRITTEN DOWN WHERE THE OFFERS ARE**, because a panel that simply lacks a control for
/// `driver` reads as *we forgot* — the same argument that kept the disabled control on screen for the
/// fortnight this hook took to land.
///
/// The classes are `NOTE` §6.5's, plus one this parcel added because §6.5 could not have known it:
/// **wrong channel**. The two fields the switchboard design's §5.2 promised by name are in it.
pub const NOT_OFFERED: &[NotOffered] = &[
    NotOffered {
        field: "driver",
        class: "wrong channel",
        why: "it is a BgAnim band-record field, not a parallax-config one, and this hook is the parallax \
              channel's. An act's BgAnim_Table is ROM and NO RAM copy of it exists in any shape, so there \
              is nothing to edit in place on that channel. The switchboard design's 5.2 promised this \
              field by name when the hook landed; the hook that landed reaches a different struct. \
              Closing it would take a BgAnim scratch of the same shape as this one, which is an aeon ask \
              rather than a panel change",
    },
    NotOffered {
        field: "rate_shift",
        class: "wrong channel",
        why: "as driver, and for the same reason: same record, same ROM, same absent scratch",
    },
    NotOffered {
        field: "step_mask / col_shift",
        class: "geometry",
        why: "BgAnim band fields again, and refused twice over: NOTE 2 rules them out as geometry derived \
              from the art's shape, and moving either without moving the art gives a picture rather than \
              an effect",
    },
    NotOffered {
        field: "pcfg_v_factor_fg",
        class: "inert",
        why: "RESERVED, with NO runtime reader at all: the v1 pipeline always sets fg_vscroll = camY. A \
              control on it would be exactly the silent no-op NOTE 0 exists to prevent",
    },
    NotOffered {
        field: "bc_step / bc_rem / bc_span / bc_pad",
        class: "inert",
        why: "the first three are derived every frame. The curve hoist recomputes all of them into the \
              engine's shadow copy each pass, so the ROM image is always 0 and a write is overwritten \
              before it is read. bc_pad is alignment and is read by nothing at all",
    },
    NotOffered {
        field: "pcfg_transition",
        class: "inert here",
        why: "read at install time and nowhere else, and the install forces it to 1 deliberately, since a \
              scene authoring 0 would be STAGED as a lerp target instead of installed. Writing it changes \
              nothing until the next arm and breaks that one if set to 0",
    },
    NotOffered {
        field: "pcfg_band_count",
        class: "coupled",
        why: "downward only, and not worth a control. Writing it ABOVE the count the install copied makes \
              the walk read scratch bytes that were never written, because Parallax_InstallScratch copies \
              this config's own bands rather than the ceiling",
    },
    NotOffered {
        field: "pcfg_v_factor_bg / pcfg_v_center_y / pcfg_v_offset",
        class: "coupled",
        why: "the vertical mapping, and NOTHING re-derives it. Every band_top_plane was computed through \
              these three at build time and stays where the build put it, so turning one slides the \
              camera's idea of the plane against the art the layers were registered on. NOTE 6.5 calls it \
              the one place a slider produces a picture whose parts disagree",
    },
    NotOffered {
        field: "band_top_plane",
        class: "coupled",
        why: "the records must stay in strictly ascending top order, because the fill reads band i+1's \
              top as band i's end. A single-field control cannot keep that invariant",
    },
    NotOffered {
        field: "brm_hshift",
        class: "coupled",
        why: "H = 1 << brm_hshift is the remap ladder table's own geometry. Changing it without changing \
              the ladder walks off the table",
    },
    NotOffered {
        field: "pcfg_deform_table_* / brm_ladder",
        class: "pointers",
        why: "their one safe written value is 0, which is an on/off rather than a nudge. A non-table \
              address is read as 256 signed bytes: noise, not a fault, and not an effect either",
    },
];

/// ⚑ **Why the other two channels have no knobs**, in the one line the panel draws on them.
///
/// Kept beside [`NOT_OFFERED`] rather than in the renderer, for the module header's reason: a sentence
/// about the engine belongs where the engine's facts are written down. It is the same finding as the first
/// two [`NOT_OFFERED`] rows, said once for a person who is on the wrong tab rather than looking for a
/// field.
pub const WRONG_CHANNEL: &str =
    "The hook that landed is a RAM copy of a PARALLAX config, and only that struct's numbers can be \
     edited in place. A raster program is a ROM instruction stream and an act's BgAnim band table is ROM \
     with no RAM copy in any build shape, so there is nothing on either channel for a knob to write. \
     Selecting still works on all three.";

/// ⚑ **What [`hook`] found**, and every number in it was resolved or derived from the loaded listing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hook {
    /// [`SCRATCH`]'s 32-bit `rawAddr`, which is the spelling `Parallax_Current_Config` holds.
    pub scratch_raw: u32,
    /// The buffer's size in bytes: `SCRATCH_END - SCRATCH`.
    pub span: u32,
    /// `parallax_config_len` — the header's length, and band record 0's offset.
    pub header_len: u32,
    /// `MAX_PARALLAX_BANDS` — how many records the buffer is reserved for.
    pub max_bands: u32,
    /// **Derived, never transcribed**: `(span - header_len) / max_bands`. 32 on `s4.debug`, 10 on
    /// `demo.debug`. See this section's header for why a transcribed 32 would have been wrong.
    pub stride: u32,
}

impl Hook {
    /// The byte offset into the scratch that `field` occupies for `band`, given the equate's value.
    ///
    /// `band` is ignored for a [`Where::Header`] field, which is why it is not an `Option`: every caller
    /// has a band index selected and a header field simply does not read it.
    pub fn offset(&self, field: &Field, band: u32, equate: u32) -> u32 {
        match field.at {
            Where::Header => equate,
            Where::Band => self.header_len + self.stride * band + equate,
        }
    }
}

/// ⚑ **THE GATE. Two conditions, destination first**, or a refusal that reads as *your build has no
/// scratch* rather than as *this is broken*.
///
/// The ordering is the switchboard design's §5.3 discipline applied to the shape it was written about: the
/// destination is resolved before anything else is asked of the listing, so a build that carries
/// [`SCRATCH_PROC`]'s name (every release build does) and not the buffer refuses on the buffer. See this
/// section's header for the measurement.
///
/// The refusals are deliberately three rather than one, because they send a person to three different
/// places: the wrong ROM shape, a listing that predates the hook, and a buffer whose size does not
/// factor. A single *"nudging unavailable"* would send them to none of them.
pub fn hook(c: &mut impl Caller) -> Result<Hook, Refusal> {
    // ⚑ CONDITION 1, FIRST: the destination. §5.3's ordering, and the reason a release build's
    // `Parallax_InstallScratch` row cannot mislead this function.
    let scratch_raw = match resolve(c, SCRATCH) {
        Ok((_, raw)) => raw,
        Err(_) => return Err(no_scratch()),
    };
    // ⚑ CONDITION 2: the request cell. Same `if DEBUG == 1 @shape_divergent` block, asked separately —
    // a name resolving is not storage existing, and one of the two resolving is not both.
    if resolve(c, SCRATCH_ARM).is_err() {
        return Err(no_scratch());
    }
    let (_, end_raw) = resolve(c, SCRATCH_END)?;
    let header_len = equate(c, CONFIG_LEN_EQU)?;
    let max_bands = equate(c, MAX_BANDS_EQU)?;
    // The span, and the two ways it can be unusable. Both are refusals with the arithmetic in them,
    // because a reader who is told "the scratch does not factor" and not the four numbers has nothing to
    // check.
    let span = end_raw.wrapping_sub(scratch_raw);
    let coherent = span > header_len && max_bands > 0 && (span - header_len) % max_bands == 0;
    if !coherent {
        return Err(Refusal::window(
            "scratchDoesNotFactor",
            format!(
                "`{SCRATCH_END}` - `{SCRATCH}` is {span} bytes, `{CONFIG_LEN_EQU}` is {header_len} and \
                 `{MAX_BANDS_EQU}` is {max_bands}, and ({span} - {header_len}) does not divide by \
                 {max_bands}. The band-record stride is DERIVED from those three ({HOOK}, \
                 `engine/ram.emp`'s own sizing run backwards) rather than transcribed, because it is 32 \
                 bytes in s4.debug and 10 in demo.debug. A stride that did not divide exactly would be \
                 rounded, and a rounded stride writes into the middle of a field on every band but the \
                 first. Nothing was written"
            ),
            Some(
                "check that the loaded listing is the one this ROM was built with; if it is, the scratch \
                 layout has changed shape and this panel's derivation needs re-reading against \
                 `engine/ram.emp`"
                    .to_string(),
            ),
        ));
    }
    Ok(Hook {
        scratch_raw,
        span,
        header_len,
        max_bands,
        stride: (span - header_len) / max_bands,
    })
}

/// ⚑ **The refusal that must read as *your listing or your build has no scratch***, not as a defect.
///
/// The switchboard design's §5.3 paid for this wording once already: the bands-off target landed at 16:48Z
/// and the owner's window had a 14:53Z listing, so a correct refusal looked like a broken feature. The
/// same two doors are open here and a third is not — a release ROM genuinely does not have this RAM — so
/// the line says which shape has it, which symbol is missing, that the feature exists in the engine, and
/// what to do. It never says the word *broken* and never suggests an alternative address.
fn no_scratch() -> Refusal {
    Refusal::window(
        "noScratchInThisBuild",
        format!(
            "`{SCRATCH}` and `{SCRATCH_ARM}` are not both in the loaded listing, so this build has \
             nowhere for a nudge to land and nothing was written. This is not a fault: the scratch is \
             declared inside `if DEBUG == 1 @shape_divergent` ({HOOK}, `engine/ram.emp:1811`), so a \
             RELEASE build emits ZERO bytes for it and a release listing carries neither name. Selecting \
             a scene still works here (`Parallax_Current_Config` is in both shapes), and only editing \
             one's numbers needs the scratch. ⚠ `{SCRATCH_PROC}` DOES appear in a release listing with an \
             address, because its body is DEBUG-gated and an empty label collapses onto its neighbour's; \
             this panel deliberately does not resolve it, since a gate keyed on that name would offer \
             knobs on a build with no buffer behind them"
        ),
        Some(format!(
            "run the DEBUG ROM (s4.debug.bin) and load the listing that build produced. If you are \
             already on a debug build, the listing predates {HOOK}: rebuild, then load the new listing"
        )),
    )
}

/// One equate's value out of the loaded listing, by name.
///
/// `emulator/lookup_equate` (§11.36) answers `value` as a JSON **number**, and its two failure codes are
/// deliberately different — `-32012` no listing, `-32013` this listing does not publish that name — so the
/// refusal carried up is the server's own and a person can tell *you forgot `load_symbols`* from *your
/// build renamed the constant*.
fn equate(c: &mut impl Caller, name: &str) -> Result<u32, Refusal> {
    let v = c.call(
        "emulator/lookup_equate",
        serde_json::json!({ "name": name }),
    )?;
    v["value"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| {
            Refusal::local(format!(
                "the bus resolved the equate `{name}` and its `value` was {:?}, which is not a number \
                 this panel can use as an offset. Nothing was written",
                v["value"]
            ))
        })
}

/// ⚑ **Whether the scratch IS the current config**, which is the whole of *did the install take*.
///
/// `NOTE` §6.2: the arm cell is a request byte and not a status byte, and the success test is this
/// comparison. Both sides are masked to 24 bits — see this section's header for why raw is wrong for three
/// of the four spelling combinations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Installed {
    /// What `Parallax_Current_Config` holds.
    pub current: u32,
    /// [`Hook::scratch_raw`], for the sentence.
    pub scratch: u32,
    /// The request byte, read back **only** to separate *serviced and refused* from *never serviced*.
    pub arm: u8,
}

impl Installed {
    /// Masked to the 24-bit bus address on both sides, which is the comparison `NOTE` §6.2 prescribes.
    pub fn took(&self) -> bool {
        self.current & 0x00FF_FFFF == self.scratch & 0x00FF_FFFF
    }

    /// **The line a person reads to know whether a knob will do anything**, and it is one of exactly
    /// three sentences. See this section's header for the table these come from.
    pub fn line(&self) -> String {
        if self.took() {
            return format!(
                "the scratch IS the current config: `{}` holds {:#010X}, which is `{SCRATCH}`. A nudge \
                 lands on the next frame.",
                PARALLAX.selector, self.current
            );
        }
        if self.arm != 0 {
            return format!(
                "the arm was NEVER SERVICED: `{SCRATCH_ARM}` still reads {:#04X} after a frame, and the \
                 engine clears it as it services it. `Parallax_Update` did not reach its head poll: it \
                 is not running this frame at all (no act loaded, or a game with no caller for it: \
                 {HOOK} names games/demo as exactly that case). `{}` still holds {:#010X}. This is not a \
                 refusal and nothing is dirty.",
                self.arm, PARALLAX.selector, self.current
            );
        }
        format!(
            "the engine SERVICED the arm and REFUSED the install: `{SCRATCH_ARM}` was cleared and `{}` \
             still holds {:#010X} rather than `{SCRATCH}` ({:#010X}). `{SCRATCH_PROC}`'s own Out: says \
             that means either no config was active (parallax off) or the active config's \
             `pcfg_band_count` exceeds `{MAX_BANDS_EQU}`, and that NOTHING was written, because a \
             refusal never clamps.",
            PARALLAX.selector, self.current, self.scratch
        )
    }
}

/// **Arm the install, run one frame, and read back whether it took.**
///
/// ⚑ **A frame is run deliberately and it is the only way in.** `NOTE` §6.2: *"An Aether client drives a
/// bus, not a call stack: it can write a byte and run a frame and cannot force a `jsr`."* One frame is
/// enough rather than two because `Parallax_Update` polls the arm **at its head, ahead of its own config
/// select**, so the install lands on that same frame — a panel that armed and stepped one frame under the
/// opposite assumption would read the ROM pointer back and conclude the hook did nothing.
///
/// The machine is paused by the caller and the frame is run through the served method on the paused
/// machine, which is [`crate::screen_pick`]'s established shape. It is not a write to a running machine:
/// the frame is asked for, one, and the machine stops again.
pub fn arm(c: &mut impl Caller) -> Result<(Hook, Installed), Refusal> {
    let h = hook(c)?;
    // The selector's own guards, unchanged and reused: this reads and depends on
    // `Parallax_Current_Config`, so the channel's drift refusal and the cursor guard both apply.
    // ⚑ Still gated on the selector being IN the listing, which is what `available` answers; no longer
    // gated on where. The drift witness is not restated here because `arm` answers `Installed` — the
    // scratch's own numbers — and the two places this channel IS reported on, the live readback and a
    // selection's readout, both carry it.
    available(c, &PARALLAX)?;
    if let Some(r) = forbidden(cursor(c)?, SCRATCH_ARM, h.scratch_raw.wrapping_add(h.span)) {
        return Err(r);
    }
    c.call(
        "emulator/write_memory",
        serde_json::json!({ "symbol": SCRATCH_ARM, "value": 1, "width": 1 }),
    )?;
    c.call("emulator/run_frames", serde_json::json!({ "frames": 1 }))?;
    let installed = took(c, &h)?;
    Ok((h, installed))
}

/// Read the two cells [`Installed`] is made of. A pure read; no frame is run.
pub fn took(c: &mut impl Caller, h: &Hook) -> Result<Installed, Refusal> {
    let current = read_u32_at_symbol(c, PARALLAX.selector)?;
    let arm = read_u8_at_symbol(c, SCRATCH_ARM)?;
    Ok(Installed {
        current,
        scratch: h.scratch_raw,
        arm,
    })
}

/// ⚑ **What the scratch holds right now**, so the controls show the numbers they are editing rather than
/// numbers this panel remembers.
///
/// `NOTE` §6.2 asks for exactly this and says why: *"The source is NOT necessarily the config the pointer
/// held when you armed"* — the arm frame runs the section boundary check too, and
/// `Parallax_Active_Config` returns the *target* during a transition, so the hook copies whatever was
/// active when `Parallax_Update` reached the poll. *"A panel that wants to know what it is editing reads
/// the scratch after the install rather than assuming it holds the config it last displayed."*
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scratch {
    /// `pcfg_band_count` as the install left it. The bound on the band selector.
    pub band_count: u32,
    /// The raw buffer, header and the bands the count claims. Field values are read out of it by offset.
    pub raw: Vec<u8>,
}

impl Scratch {
    /// One field's current value, or `None` when the read did not reach that far.
    pub fn value(&self, h: &Hook, field: &Field, band: u32, equate: u32) -> Option<u32> {
        let off = h.offset(field, band, equate) as usize;
        let w = field.width as usize;
        if off + w > self.raw.len() {
            return None;
        }
        Some(match w {
            1 => self.raw[off] as u32,
            _ => u32::from(u16::from_be_bytes([self.raw[off], self.raw[off + 1]])),
        })
    }
}

/// **Read the scratch back**, header plus the bands `pcfg_band_count` claims.
///
/// Two reads rather than one sized guess: the count first, then exactly `header + stride * count` bytes.
/// Reading the whole reservation would be the easier call and would ask the bus for bytes the install
/// never wrote — `Parallax_InstallScratch` copies *"the header plus this config's OWN bands, not the
/// ceiling"* — and showing those as field values would be this panel presenting uninitialised RAM as a
/// scene's numbers.
///
/// A count above [`Hook::max_bands`] is **reported, not trusted**: it is clamped for the read and the
/// count is carried through unchanged, so the caller can say the buffer disagrees with the engine's own
/// ceiling instead of reading past the span.
pub fn read_scratch(c: &mut impl Caller, h: &Hook) -> Result<Scratch, Refusal> {
    let head = read_bytes_at_symbol(c, SCRATCH, h.header_len as usize)?;
    let count_off = equate(c, &format!("{HEADER_EQU_PREFIX}pcfg_band_count"))? as usize;
    let band_count = *head.get(count_off).ok_or_else(|| {
        Refusal::local(format!(
            "`{HEADER_EQU_PREFIX}pcfg_band_count` resolves to offset {count_off}, which is outside the \
             {} header bytes `{CONFIG_LEN_EQU}` claims. Nothing was decoded",
            h.header_len
        ))
    })? as u32;
    let read_bands = band_count.min(h.max_bands);
    let want = (h.header_len + h.stride * read_bands).min(h.span) as usize;
    let raw = read_bytes_at_symbol(c, SCRATCH, want)?;
    Ok(Scratch { band_count, raw })
}

/// ⚑ **Turn one knob**, or refuse and say which of the five reasons it is.
///
/// The order the guards run in, and each one exists because of a specific way this could lie:
///
/// 1. **[`hook`]** — this build has the buffer and its layout factors.
/// 2. **The install** — [`Installed::took`]. A write into a scratch the engine is not reading is the
///    silent no-op this whole module exists against, and a section crossing produces it without anybody
///    doing anything wrong ([`NOTE`] §6.6 consequence 1: `Parallax_CheckBoundary` evicts the scratch).
/// 3. **The band index**, against `pcfg_band_count` as the install actually left it — not against
///    [`Hook::max_bands`], which is the reservation and not the scene.
/// 4. **The value**, against [`Field::range`]. The door accepts every byte; only some of them mean
///    something.
/// 5. **The offset**, against [`Hook::span`], and [`forbidden`] on the resolved destination — with the
///    cursor itself resolved from the loaded listing ([`cursor`]) rather than transcribed. The span
///    check is the one that makes a per-game stride safe: a band index inside the count whose record
///    still fell outside the buffer would be a write into whatever RAM follows.
pub fn nudge(
    c: &mut impl Caller,
    field: &Field,
    band: u32,
    value: u32,
) -> Result<(Hook, String), Refusal> {
    let h = hook(c)?;
    let state = took(c, &h)?;
    if !state.took() {
        return Err(Refusal::window(
            "scratchNotInstalled",
            format!(
                "nothing was written, because a write would have done NOTHING: {}",
                state.line()
            ),
            Some(format!(
                "arm the scratch first. If it was armed and has stopped being the current config, the \
                 camera crossed a section boundary. `Parallax_CheckBoundary` installs the new section's \
                 own ROM preset, exactly as it always did, so arm it again ({HOOK})"
            )),
        ));
    }
    let s = read_scratch(c, &h)?;
    if matches!(field.at, Where::Band) && band >= s.band_count {
        return Err(Refusal::window(
            "bandOutsideConfig",
            format!(
                "band {band} does not exist in the installed config: its `pcfg_band_count` is {}, and \
                 `{SCRATCH_PROC}` copies the header plus THIS config's own bands rather than the \
                 `{MAX_BANDS_EQU}` ceiling ({}). The bytes at band {band} were never written by the \
                 install, so a value here would be edited into uninitialised RAM and read by nothing. \
                 Nothing was written",
                s.band_count, h.max_bands
            ),
            Some("pick a band below the installed count, or arm a scene with more bands".to_string()),
        ));
    }
    let (lo, hi) = field.range;
    if value < lo || value > hi {
        return Err(Refusal::window(
            "valueOutsideRange",
            format!(
                "{value} is outside {lo}..={hi}, which is what `{}` means something over: {}. The door \
                 would accept it and the engine would read it, which is why this is refused here rather \
                 than clipped. Nothing was written",
                field.label, field.what
            ),
            None,
        ));
    }
    let equ = equate(c, &field.equate_name())?;
    let off = h.offset(field, band, equ);
    if off + u32::from(field.width) > h.span {
        return Err(Refusal::window(
            "offsetOutsideScratch",
            format!(
                "`{}` for band {band} resolves to offset {off} + {} bytes, and the scratch is only {} \
                 bytes ({SCRATCH_END} - {SCRATCH}). Writing it would land in whatever RAM follows the \
                 buffer. Nothing was written",
                field.label, field.width, h.span
            ),
            None,
        ));
    }
    if let Some(r) = forbidden(cursor(c)?, SCRATCH, h.scratch_raw.wrapping_add(off)) {
        return Err(r);
    }
    let mut req = serde_json::json!({
        "symbol": SCRATCH,
        "value": value,
        "width": field.width,
    });
    if off > 0 {
        req["disp"] = serde_json::json!(off);
    }
    c.call("emulator/write_memory", req)?;
    Ok((
        h,
        format!(
            "{}{} = {value} at `{SCRATCH}`+{off}, which the next frame reads. {}",
            field.label,
            match field.at {
                Where::Band => format!(" on band {band}"),
                Where::Header => String::new(),
            },
            field.what
        ),
    ))
}

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
        // ⚑ The RAW spelling, because both things done with it next, `forbidden` and
        // `Channel::witness`, compare against addresses in that spelling: `forbidden` against the
        // cursor as the listing resolves it, and the witness against the note's transcription.
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

// -------------------------------------------------------------------------------------------------------
// ⚑ THE LAYOUT GATE: the fact a refusal is allowed to stand on
// -------------------------------------------------------------------------------------------------------

/// What the loaded listing puts at or before `addr24`: the label, and where that label **starts**.
///
/// ⚑ **The start is computed as `queried − disp`, not read from the reply's `addr`.** `disp` is REQUIRED
/// on the address direction (§4) and `addr` on that branch is the *symbol's* address, so the two agree —
/// but subtracting the displacement is the spelling that cannot be confused with the 32-bit `rawAddr`,
/// and it keeps this probe in the same 24-bit space [`resolve`] hands back.
///
/// ⚑ **`None` means THE PROBE DID NOT ANSWER, and it is not *the listing names nothing there*.** The
/// distinction is worth the sentence because the second reading is impossible: this is only ever called
/// with an address at or past a symbol [`resolve`] just resolved, so a listing that answered the resolve
/// always has something at or before it. What is left is a bus that failed the call or a reply with no
/// `name` in it. [`layout`] turns that into a stated limit rather than a pass or a refusal: a transport
/// hiccup must not refuse a healthy build, and must not read as a check that succeeded either.
fn label_at(c: &mut impl Caller, addr24: u32) -> Option<(String, u32)> {
    let v = c
        .call(
            "emulator/lookup_symbol",
            serde_json::json!({ "addr": format!("0x{addr24:06X}") }),
        )
        .ok()?;
    let name = v["name"].as_str()?.to_string();
    let disp = u32::try_from(v["disp"].as_u64().unwrap_or(0)).ok()?;
    Some((name, addr24.checked_sub(disp)?))
}

/// ⚑ **THE GATE. It stands on the LAYOUT the loaded listing describes, never on an address.**
///
/// The module header has the reasoning; this is what it checks, and both facts are re-derived from the
/// listing on every gesture:
///
/// 1. **Every cell's bytes lie inside the symbol the cell names.** For each cell, the last byte it
///    writes — `symbol + disp + width - 1` — must still have that same symbol as its nearest preceding
///    label. Equivalently: **no other symbol starts inside the bytes this cell writes.** That is the
///    direct test of a transcribed `disp`, and `BgAnim_LastStep+4` is the one in this module.
/// 2. **An array a write-set poisons whole is still exactly that large, and still partitions.** See
///    [`Covering`]. Two probes pin the extent — nothing starts inside the covered bytes (1, above), and
///    something starts exactly one past them — and the listing's own count equate divides it.
///
/// ⚑ **Address-invariant by construction.** aeon's `61918621` slid six symbols by `+$20` and one by
/// `+$200` and changed no width; every comparison here is between two addresses from the *same* listing,
/// so a uniform slide cancels and this gate stays silent — which is the whole reason it replaced the
/// equality test that refused every raster and bands gesture on a healthy build.
///
/// Returns one line per fact **established or unmeasurable**, for the gesture's readout. A fact it
/// cannot measure is stated (the `⚠` lines), never counted as a pass: an absence is not a finding.
///
/// ⚑ **Run in full BEFORE the first cell is written**, not folded into [`run`]'s loop. A gate that
/// checked cell 3's layout after cells 1 and 2 had landed would leave the channel half set on exactly
/// the finding it exists to catch.
pub fn layout(c: &mut impl Caller, channel: &Channel) -> Result<Vec<String>, Refusal> {
    let mut said = Vec::new();
    // ⚑ Cells whose probe did not answer. Counted rather than merely reported, because the account
    // below would otherwise end "and it is" under a line saying a check did NOT run, which is the
    // silent-pass shape this whole gate exists against.
    let mut unmeasured = 0usize;

    // 1. Every cell stays inside the symbol it names.
    for cell in channel.writes {
        let (addr, _) = resolve(c, cell.symbol)?;
        let span = cell.disp + u32::from(cell.width);
        let last = addr + span - 1;
        match label_at(c, last) {
            Some((_, start)) if start == addr => {}
            Some((other, start)) => {
                return Err(Refusal::window(
                    "cellLeavesItsSymbol",
                    format!(
                        "`{}`{} is {} byte{} wide, so it writes through {last:#010X}, and the loaded \
                         listing starts `{other}` at {start:#010X}, INSIDE those bytes. This cell's \
                         offset is transcribed from {ENGINE} ({}), so the struct this write-set was \
                         written against is not the struct this build has, and the write would land \
                         partly in `{other}`. Nothing was written.\n\nThis is a LAYOUT finding, not a \
                         moved address: every address compared here came out of the one loaded \
                         listing, so a build that merely slid its RAM would not raise it",
                        cell.symbol,
                        if cell.disp > 0 {
                            format!("+{}", cell.disp)
                        } else {
                            String::new()
                        },
                        cell.width,
                        if cell.width == 1 { "" } else { "s" },
                        cell.why,
                    ),
                    Some(format!(
                        "re-read {ENGINE}'s installer for this channel against the build you loaded: \
                         the cell's offset or width has to change, and re-transcribing an address will \
                         not help"
                    )),
                ));
            }
            None => {
                unmeasured += 1;
                said.push(format!(
                    "⚠ `{}` + {span}: the bus did not answer what the loaded listing puts at or before \
                     {last:#010X}, so the gate could NOT check that this cell stays inside the symbol \
                     it names, and did not. The write was not refused for it: a probe that failed to \
                     answer is not evidence of a layout fault",
                    cell.symbol
                ));
            }
        }
    }
    if unmeasured > 0 {
        said.push(format!(
            "⚠ layout: {unmeasured} of {} cells could not be placed in the loaded listing, so this \
             account is INCOMPLETE. The cells that were placed are inside the symbols they name; \
             nothing is claimed about the rest",
            channel.writes.len(),
        ));
    } else if channel.writes.iter().all(|w| w.disp == 0) {
        said.push(format!(
            "layout: {} cell{} checked against the loaded listing, with no transcribed field offset \
             among them. Every one is a whole symbol at displacement 0, so the only layout fact this \
             channel depends on is that each symbol is still at least as wide as the write, and it is",
            channel.writes.len(),
            if channel.writes.len() == 1 { "" } else { "s" },
        ));
    } else {
        said.push(format!(
            "layout: {} cells checked against the loaded listing; no other symbol starts inside the \
             bytes any of them writes, so the transcribed displacements still address the struct this \
             build has",
            channel.writes.len(),
        ));
    }

    // 2. An array the write-set poisons whole is still exactly that large, and still partitions.
    for cov in channel.covers {
        let (addr, _) = resolve(c, cov.symbol)?;
        // Which bytes of `cov.symbol` this write-set actually covers. A set rather than a maximum,
        // because a write-set with a HOLE in it poisons the same top byte while leaving a band alive.
        let covered: u32 = channel
            .writes
            .iter()
            .filter(|w| w.symbol == cov.symbol)
            .map(|w| w.disp + u32::from(w.width))
            .max()
            .unwrap_or(0);
        let mut hit = vec![false; covered as usize];
        for w in channel.writes.iter().filter(|w| w.symbol == cov.symbol) {
            for b in w.disp..w.disp + u32::from(w.width) {
                hit[b as usize] = true;
            }
        }
        if let Some(hole) = hit.iter().position(|h| !h) {
            return Err(Refusal::window(
                "poisonHasAHole",
                format!(
                    "this write-set reaches byte {} of `{}` but leaves byte {hole} unwritten. {} \
                     Nothing was written",
                    covered - 1,
                    cov.symbol,
                    cov.why
                ),
                Some(format!(
                    "re-read {ENGINE}'s installer: it poisons the array whole"
                )),
            ));
        }

        // The extent, pinned rather than believed: (1) above proved nothing starts inside the covered
        // bytes; this proves something starts exactly one past them, so the array is not larger.
        // ⚑ Whether the extent was actually pinned, so the partition line below cannot assert
        // "exactly the N bytes this write-set poisons" on the strength of a probe that did not answer.
        let mut pinned = true;
        match label_at(c, addr + covered) {
            Some((_, start)) if start != addr => {}
            Some((_, _)) => {
                return Err(Refusal::window(
                    "arrayOutgrewItsPoison",
                    format!(
                        "this write-set poisons {covered} bytes of `{}`, and the loaded listing \
                         continues that symbol past them: nothing else starts at {:#010X}. {} So the \
                         indices past the {covered}th byte would keep their stale state and the switch \
                         would stop being atomic, silently, with every cell still landing inside its \
                         own symbol. Nothing was written.\n\nThis is a LAYOUT finding: it compares two \
                         addresses from the one loaded listing, so a build that merely slid its RAM \
                         would not raise it. ⚑ It reads the same way for an array that is the LAST \
                         symbol in the listing, which this gate cannot distinguish and does not guess",
                        cov.symbol,
                        addr + covered,
                        cov.why,
                    ),
                    Some(format!(
                        "re-read {ENGINE}'s installer for this channel: the write-set has to cover the \
                         array this build has"
                    )),
                ));
            }
            None => {
                pinned = false;
                said.push(format!(
                    "⚠ `{}`: the bus did not answer what the loaded listing puts at or before \
                     {:#010X}, so the gate could NOT pin this array's extent and did not check that \
                     the write-set covers all of it",
                    cov.symbol,
                    addr + covered
                ));
            }
        }

        // The published count, which turns the byte figure into the figure the engine reasons in. A
        // missing or non-dividing equate is a STATED LIMIT and never a refusal: nothing is computed
        // from it, so an unmeasurable partition costs the gesture nothing but silence would cost the
        // reader the one number that says what the poison covers.
        match equate(c, cov.count_equate).ok() {
            Some(n) if n > 0 && covered.is_multiple_of(n) => said.push(if pinned {
                format!(
                    "layout: `{}` is exactly the {covered} bytes this write-set poisons, and the \
                     listing's own `{}` = {n} partitions them into {n} {} slots of {} bytes each, so \
                     every {} the build has is poisoned",
                    cov.symbol,
                    cov.count_equate,
                    cov.element,
                    covered / n,
                    cov.element,
                )
            } else {
                format!(
                    "⚠ this write-set poisons {covered} bytes of `{}` and the listing's own `{}` = {n} \
                     divides them into {n} {} slots of {} bytes, but the array's extent went unpinned \
                     above, so it is NOT established that {covered} bytes is all of it",
                    cov.symbol,
                    cov.count_equate,
                    cov.element,
                    covered / n,
                )
            }),
            Some(n) => said.push(format!(
                "⚠ `{}` is exactly the {covered} bytes this write-set poisons, but the listing's `{}` = \
                 {n} does not divide them. The per-{} size could not be derived, so the gate checked \
                 the EXTENT and did NOT check that the poison reaches every {}",
                cov.symbol, cov.count_equate, cov.element, cov.element
            )),
            None => said.push(format!(
                "⚠ `{}` is exactly the {covered} bytes this write-set poisons, but the loaded listing \
                 does not publish `{}`, so the gate could not say how many {} slots that is",
                cov.symbol, cov.count_equate, cov.element
            )),
        }
    }
    Ok(said)
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
    /// ⚑ **What [`layout`] established, and what it could not** — one line each, drawn under the cells.
    /// The `⚠` lines are facts the gate did not check, stated rather than passed over.
    pub layout: Vec<String>,
    /// ⚑ **The note-versus-listing witness** ([`Drift`]), `Some` only when they disagree. It did not
    /// block this gesture and says so; every cell above was addressed by name and resolved by the
    /// server out of the loaded listing.
    pub drift: Option<Drift>,
}

impl Wrote {
    /// The headline: what moved and by how many cells. The count is stated because the card promised one
    /// and the engine wanted more, and a reader who sees four writes go past deserves to know that is the
    /// design rather than a bug.
    pub fn line(&self) -> String {
        let head = format!(
            "pointed at {}. {} written, which is {}.",
            self.target,
            plural(self.cells.len(), "cell", "cells"),
            self.installer
        );
        // ⚑ The witness rides the headline rather than a separate control, because a line drawn only
        // when it has something to say cannot become the standing caveat this tree forbids, and because
        // the moment a person is owed the drift is the moment they made the panel write.
        match &self.drift {
            Some(d) => format!("{head} {}", d.line()),
            None => head,
        }
    }
}

/// **Point `channel` at `target`, by running its whole write-set.**
///
/// The order the guards run in:
///
/// 1. **[`available`]** — the channel's live cell resolves in this build's listing at all.
/// 2. **[`cursor`]**, resolving the START chord's cursor out of the loaded listing ONCE for the whole
///    gesture, so every cell below is measured against one listing rather than re-resolved per cell.
/// 3. **[`forbidden`]**, on the live cell's name and resolved address independently.
/// 4. **[`layout`]**, refusing when the loaded listing's own layout no longer matches the struct this
///    write-set was transcribed against. ⚑ This is where step 4 used to be a `Channel::drift`
///    refusal on an address, which refused every raster and every bands gesture on a healthy build;
///    see the module header. The note survives as [`Wrote::drift`], stated and not blocking.
/// 5. **Resolve the target by name**, so the value written is the listing's and not this crate's.
/// 6. **Every cell**, each addressed `{symbol, disp, value, width}` so the server resolves the
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
    // Resolved ONCE per gesture and handed down, so every cell of the write-set is measured against the
    // same listing. Resolving it per cell would let a listing swapped mid-set be checked two ways.
    let cur = cursor(c)?;
    if let Some(r) = forbidden(cur, channel.selector, sel_addr) {
        return Err(r);
    }
    let said = layout(c, channel)?;
    let drift = witnessed(c, channel, sel_addr);
    let (_, value) = resolve(c, target)?;
    run(c, cur, channel, target, value, said, drift)
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
    let cur = cursor(c)?;
    if let Some(r) = forbidden(cur, channel.selector, sel_addr) {
        return Err(r);
    }
    let said = layout(c, channel)?;
    let drift = witnessed(c, channel, sel_addr);
    // The empty program or empty table, when the listing has it. **No fallback**: a panel that reached
    // for a second address when the first was missing would be choosing a target in somebody else's RAM.
    match resolve(c, symbol) {
        Ok((_, value)) => run(c, cur, channel, symbol, value, said, drift),
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
    cursor: Cursor,
    channel: &Channel,
    target: &str,
    target_value: u32,
    said: Vec<String>,
    drift: Option<Drift>,
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
        if let Some(r) = forbidden(cursor, cell.symbol, raw.wrapping_add(cell.disp)) {
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
        layout: said,
        drift,
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
    /// ⚑ **The note-versus-listing witness** ([`Drift`]), `Some` only when they disagree. The readback
    /// above came from the address the **listing** resolves, which is the cell the engine reads; the
    /// witness says so and names the note's number so the gap is visible rather than silent.
    pub drift: Option<Drift>,
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
        // ⚑ The witness is appended to whichever finding follows, never printed instead of it: a zero
        // selector and a drifted note are two separate facts and the reader is owed both.
        let witness = match &self.drift {
            Some(d) => format!(" {}", d.line()),
            None => String::new(),
        };
        let selector = channel.selector;
        // ⚑ A zero is a **stated per-channel finding**, never "the listing does not name it". See
        // [`Channel::zero`]: on the raster channel it is a documented off state, and on the band channel
        // it is a fault. Reporting both as an unnamed address would hide one and alarm about the other.
        if self.value == 0 {
            return format!("{selector} holds 0. {}.{witness}", channel.zero);
        }
        match (&self.name, self.disp) {
            (Some(n), 0) => format!(
                "{selector} holds {:#010X}, which is `{n}`.{witness}",
                self.value
            ),
            (Some(n), d) => format!(
                "{selector} holds {:#010X}, which is ${d:X} past `{n}` and so is probably not `{n}` at \
                 all: the listing carries no sizes.{witness}",
                self.value
            ),
            (None, _) => format!(
                "{selector} holds {:#010X}, which the loaded listing does not name.{witness}",
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
    // ⚑ **The witness rides the READ as well as the write**, which is a coherence property rather than
    // caution and it used to be a refusal on both. The refusal's old argument was that a panel which
    // will not write a cell it cannot place should not then print a confident sentence about that same
    // cell's contents. The premise was wrong in both halves: the read is at the address the LISTING
    // resolves, which is the cell the engine reads, so the sentence is about the right four bytes — and
    // refusing here blanked the raster and bands readbacks on a healthy build. What both paths share
    // now is the statement, not the veto.
    let sel = available(c, channel)?;
    let drift = witnessed(c, channel, sel);
    let value = read_u32_at_symbol(c, channel.selector)?;
    if value == 0 {
        return Ok(Live {
            value,
            name: None,
            disp: 0,
            drift,
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
            drift,
        },
        Err(_) => Live {
            value,
            name: None,
            disp: 0,
            drift,
        },
    })
}

/// **The band table as the machine holds it right now**, read through the selector rather than through an
/// address this crate carries.
pub fn read_bands(c: &mut impl Caller) -> Result<Bands, Refusal> {
    let sel = available(c, &BANDS)?;
    let drift = witnessed(c, &BANDS, sel);
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
    let mut out = bands(&raw);
    // ⚑ **The witness and the stated limit, folded into the caveat this readout already has** rather
    // than drawn as a standing banner — §11.27 forbids the unconditional caveat, and a limit that is
    // true of every decode would become one. `Bands::caveat` is conditional on a readback having been
    // asked for, which is exactly when the limit is load-bearing.
    //
    // The limit is real and this is the channel that has it: `BAND_RECORD_BYTES` is **transcribed**
    // from `NOTE` §2 and the loaded listing publishes no equate for it (measured 2026-09-19: the only
    // `bg_anim` layout equate in `s4.debug.lst` is `BGANIM_MAX_BANDS`; the nine `band_entry_*` rows
    // belong to the parallax band record, a different struct). So `layout` has nothing to check the
    // record stride against, and saying so is the honest state — not a silent pass.
    let notes: Vec<String> = out
        .caveat
        .take()
        .into_iter()
        .chain(drift.as_ref().map(Drift::line))
        .chain(std::iter::once(format!(
            "LIMIT: the {BAND_RECORD_BYTES}-byte record stride and the {BAND_COUNT_BYTES}-byte count \
             word above are TRANSCRIBED from {NOTE} §2, and the loaded listing publishes no equate for \
             either, so nothing checked them against this build. `{BGANIM_MAX_BANDS_EQU}` is the only \
             bg_anim layout equate the listing carries, and it bounds the band COUNT, not the stride."
        )))
        .collect();
    out.caveat = Some(notes.join(" "));
    Ok(out)
}

/// One longword out of the location `symbol` names, through the served reader.
fn read_u32_at_symbol(c: &mut impl Caller, symbol: &str) -> Result<u32, Refusal> {
    let r = c.call(
        "emulator/read_memory",
        serde_json::json!({ "symbol": symbol, "len": 4 }),
    )?;
    parse_hex_bytes(&r, 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// One byte out of the location `symbol` names, through the served reader.
fn read_u8_at_symbol(c: &mut impl Caller, symbol: &str) -> Result<u8, Refusal> {
    let r = c.call(
        "emulator/read_memory",
        serde_json::json!({ "symbol": symbol, "len": 1 }),
    )?;
    parse_hex_bytes(&r, 1).map(|b| b[0])
}

/// `len` bytes from the location `symbol` names.
///
/// By name rather than by address for the module header's reason, and it applies to reads as loudly as to
/// writes: the server resolves the destination out of the table `emulator/load_symbols` bound, so a read
/// cannot land in RAM a carried address has stopped naming.
fn read_bytes_at_symbol(c: &mut impl Caller, symbol: &str, len: usize) -> Result<Vec<u8>, Refusal> {
    let r = c.call(
        "emulator/read_memory",
        serde_json::json!({ "symbol": symbol, "len": len }),
    )?;
    parse_hex_bytes(&r, len)
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
    /// ⚑ **What [`layout`] established before the first cell, and what it could not** ([`Wrote::layout`]).
    /// Empty on a refusal for the same reason [`Readout::cells`] is: nothing ran.
    pub layout: Vec<String>,
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
    /// ⚑ **The nudge surface's state**, and it is deliberately `None` until a gesture asks.
    ///
    /// The gate ([`hook`]) costs four lookups, and running it every frame to decide whether to grey a
    /// control would put four bus calls in a draw path for an answer that changes only when a listing is
    /// loaded. So the controls are drawn live and the gate answers on the gesture, which is the shape
    /// [`Off::At`]'s button already uses for the same reason: whether a symbol is in this build's listing
    /// is a question only the bus can answer, and the answer is the refusal the gesture prints.
    nudge: Option<Result<Nudging, String>>,
    /// Which band record the band-scoped controls address. Bounded on use, never on entry: a stale index
    /// from a wider scene is refused with the count in the sentence rather than silently moved.
    band_index: u32,
}

/// **The nudge surface as one gesture left it**: what the build has, whether the scratch is installed, and
/// the numbers it currently holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Nudging {
    /// What [`hook`] resolved and derived.
    pub hook: Hook,
    /// Whether the scratch is the current config, and which of the three states it is in.
    pub installed: Installed,
    /// The scratch's own bytes, when it is installed. `None` when it is not — there is nothing coherent to
    /// show, and showing the buffer's stale contents as the scene's numbers would be a readout that lies.
    pub scratch: Option<Scratch>,
    /// Each offered field's resolved offset, so the draw path spells no offset and runs no lookup.
    /// `(key, offset_equate_value)`.
    pub offsets: Vec<(&'static str, u32)>,
}

impl Nudging {
    /// The equate value [`hook`] resolved for `field`, or `None` when this listing did not publish it.
    pub fn equate_of(&self, field: &Field) -> Option<u32> {
        self.offsets
            .iter()
            .find(|(k, _)| *k == field.key)
            .map(|(_, v)| *v)
    }

    /// One offered field's current value, as the scratch holds it.
    pub fn value_of(&self, field: &Field, band: u32) -> Option<u32> {
        let s = self.scratch.as_ref()?;
        s.value(&self.hook, field, band, self.equate_of(field)?)
    }

    /// **The derivation, said out loud**, because a stride nobody can see is a stride nobody can check.
    ///
    /// ⚑ **It also states when the listing has moved past [`SCRATCH_NOTED_ADDR`], and says that is fine.**
    /// A reader who compares this panel against `NOTE` §6.1 will find the two addresses disagree. When
    /// this was written every other symbol in the module answered such a disagreement with a refusal,
    /// and this line existed to say why this one did not; [`Channel::witness`] has since made *stating
    /// it* the rule rather than the exception, and the line is kept because the reason it gives is the
    /// specific one: the symbol is at the RAM tail inside a size-varying `@shape_divergent` group and
    /// moves on ordinary aeon commits.
    pub fn shape_line(&self) -> String {
        let mut s = format!(
            "the scratch is {} bytes at {:#010X}: a {}-byte header and {} records of {} bytes. The stride \
             is DERIVED from those ({} - {}) / {}, not transcribed: it is 32 on s4.debug and 10 on \
             demo.debug.",
            self.hook.span,
            self.hook.scratch_raw,
            self.hook.header_len,
            self.hook.max_bands,
            self.hook.stride,
            self.hook.span,
            self.hook.header_len,
            self.hook.max_bands,
        );
        if self.hook.scratch_raw != SCRATCH_NOTED_ADDR {
            s.push_str(&format!(
                " {NOTE} records it at {SCRATCH_NOTED_ADDR:#010X}; this is a RAM-tail symbol in a \
                 size-varying group, so a moved address is an ordinary rebuild rather than the drift a \
                 selector's would be, and nothing refuses on it."
            ));
        }
        s
    }
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
            nudge: None,
            band_index: 0,
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

    /// The nudge surface as the last gesture left it, or the refusal that replaced it. `None` before any
    /// gesture — *not looked yet*, which the panel says rather than drawing an empty state that reads as
    /// *there is nothing*.
    pub fn nudging(&self) -> Option<Result<&Nudging, &String>> {
        self.nudge.as_ref().map(Result::as_ref)
    }

    /// The band record the band-scoped controls address.
    pub fn band_index(&self) -> u32 {
        self.band_index
    }

    /// Move the band cursor. Bounded at [`Hook::max_bands`] when the gate has answered, so the control
    /// cannot offer a record the buffer has no room for; the tighter bound (the installed
    /// `pcfg_band_count`) is enforced in [`nudge`] with the count in the sentence, because a cursor that
    /// silently snapped would hide a scene having fewer bands than the last one.
    pub fn look_at_band(&mut self, i: u32) {
        let ceiling = match self.nudge.as_ref() {
            Some(Ok(n)) => n.hook.max_bands.saturating_sub(1),
            _ => u32::MAX,
        };
        self.band_index = i.min(ceiling);
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
                layout: Vec::new(),
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
                    layout: Vec::new(),
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
                    layout: Vec::new(),
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
                    layout: Vec::new(),
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
                    layout: w.layout,
                    refused: false,
                }
            }
            // ⚑ A refusal does NOT record a change. The standing statement names what is in effect, and a
            // write that was refused is not in effect: recording it would make the loudest line on this
            // panel the one that is wrong.
            Err(e) => Readout {
                head: refusal_line(&e, what),
                cells: Vec::new(),
                layout: Vec::new(),
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

    // ---------------------------------------------------------------------------------------------
    // ⚑ The nudge gestures. Both write, so both are pause-write-resume through `gesture_nudge`.
    // ---------------------------------------------------------------------------------------------

    /// **Arm the install**: write the request byte, run one frame, read back whether it took.
    ///
    /// It writes, so it is paused. It also runs a frame *while paused*, which is the only way in — see
    /// [`arm`] — and the frame is asked for through the served method, one, on the machine the caller
    /// stopped. [`crate::screen_pick::paused_for`] puts the run state back afterwards exactly as it does
    /// for a selection.
    pub fn arm_scratch(
        &mut self,
        machine: &mut crate::machine::Machine,
        bus: &mut crate::bus::Bus,
    ) {
        self.gesture_nudge(machine, bus, "arm the scratch", |c| {
            let (hook, installed) = arm(c)?;
            let scratch = installed
                .took()
                .then(|| read_scratch(c, &hook))
                .transpose()?;
            Ok(Nudging {
                hook,
                installed,
                scratch,
                offsets: resolved_offsets(c, &hook),
            })
        });
    }

    /// **Re-read the nudge surface** without arming: the gate, the install state, and the numbers.
    ///
    /// Separate from [`Panel::arm_scratch`] because they are different questions and conflating them would
    /// make *"has this build got the hook?"* unanswerable without changing the machine. This one runs no
    /// frame and writes nothing, so it does not pause.
    pub fn refresh_nudge(
        &mut self,
        machine: &mut crate::machine::Machine,
        bus: &mut crate::bus::Bus,
    ) {
        let sys = machine.system_mut();
        let mut c = crate::screen_pick::PlayerCaller { bus, sys };
        self.nudge = Some(
            (|| {
                let hook = hook(&mut c)?;
                let installed = took(&mut c, &hook)?;
                let scratch = installed
                    .took()
                    .then(|| read_scratch(&mut c, &hook))
                    .transpose()?;
                Ok(Nudging {
                    hook,
                    installed,
                    scratch,
                    offsets: resolved_offsets(&mut c, &hook),
                })
            })()
            .map_err(|e: Refusal| refusal_line(&e, "read the nudge surface")),
        );
    }

    /// **Turn one knob.** Pause, write the one byte or word, resume, then re-read the scratch.
    ///
    /// The re-read is part of the gesture rather than a separate button, and it is the same argument the
    /// selection path makes in reverse. A selection *retires* its readback because the swap it asked for
    /// has not happened yet; a nudge's write has already landed in RAM the moment the door returns, so
    /// re-reading shows the person the number they now have. A control left showing its own last keystroke
    /// would be an echo, which is what this module is written against.
    pub fn nudge_field(
        &mut self,
        machine: &mut crate::machine::Machine,
        bus: &mut crate::bus::Bus,
        field: &'static Field,
        value: u32,
    ) {
        let band = self.band_index;
        let what = format!("nudge {} to {value}", field.label);
        self.gesture_nudge(machine, bus, &what, move |c| {
            let (hook, _line) = nudge(c, field, band, value)?;
            let installed = took(c, &hook)?;
            let scratch = installed
                .took()
                .then(|| read_scratch(c, &hook))
                .transpose()?;
            Ok(Nudging {
                hook,
                installed,
                scratch,
                offsets: resolved_offsets(c, &hook),
            })
        });
    }

    /// ⚑ **The one pause-write-resume path for the nudge surface**, [`Panel::gesture`]'s twin.
    ///
    /// Not merged with it, and the reason is the return type rather than taste: [`Panel::gesture`]'s body
    /// yields a [`Wrote`] and records a [`Change`] against a channel, and neither is right here. A nudge
    /// is not a selection — nothing was *pointed* anywhere, and recording one as a change would make the
    /// standing statement name a target that was never chosen. What the two DO share is
    /// [`crate::screen_pick::paused_for`], which both call rather than re-implement, so *"the machine
    /// really was paused while the body ran"* has one implementation in this crate and not two.
    ///
    /// ⚑ **A nudge does not enter the standing statement, and that is a decision rather than an
    /// omission.** The statement names *what this panel has pointed somewhere and not put back*, and it is
    /// drawn in the warning colour above everything. A turned knob is a state change too, but it is a
    /// change **to a buffer this panel installed and names on screen** — the install is what the statement
    /// would be about, and [`Installed::line`] says it in the surface's own words, permanently, where the
    /// knobs are. Adding every knob to the top line would push the selections out of the eye's way with
    /// rows that repeat what the control beside them already shows.
    fn gesture_nudge(
        &mut self,
        machine: &mut crate::machine::Machine,
        bus: &mut crate::bus::Bus,
        what: &str,
        body: impl FnOnce(&mut crate::screen_pick::PlayerCaller<'_>) -> Result<Nudging, Refusal>,
    ) {
        let outcome = crate::screen_pick::paused_for(machine, bus, |machine, bus| {
            let sys = machine.system_mut();
            body(&mut crate::screen_pick::PlayerCaller { bus, sys })
        });
        let (result, run) = match outcome {
            Ok(both) => both,
            Err(why) => {
                self.run = None;
                self.last = Some(Readout {
                    head: format!(
                        "the window could not pause the machine to {what}, so nothing was written. {why}"
                    ),
                    cells: Vec::new(),
                    layout: Vec::new(),
                    refused: true,
                });
                return;
            }
        };
        self.run = Some(run);
        self.last = Some(match &result {
            Ok(n) => Readout {
                head: format!("{what}: {}", n.installed.line()),
                cells: Vec::new(),
                layout: Vec::new(),
                refused: false,
            },
            Err(e) => Readout {
                head: refusal_line(e, what),
                cells: Vec::new(),
                layout: Vec::new(),
                refused: true,
            },
        });
        // ⚑ **A refusal REPLACES the surface rather than leaving the last good one standing.** The
        // refusals here are all about the surface itself — no buffer in this build, the scratch evicted by
        // a section crossing, a layout that does not factor — so keeping the previous `Nudging` beside one
        // would draw live-looking numbers for a buffer the panel has just been told it cannot reach.
        self.nudge = Some(result.map_err(|e| refusal_line(&e, what)));
    }
}

/// Resolve every [`FIELDS`] entry's offset equate once, for a draw path that must spell none of them.
///
/// A field whose equate this listing does not publish is **left out**, not defaulted to zero: offset 0 is
/// `pcfg_band_count`, so a default would turn an unpublished equate into a write at the band count. The
/// control for it then draws as unavailable, which is the true statement.
fn resolved_offsets(c: &mut impl Caller, _h: &Hook) -> Vec<(&'static str, u32)> {
    FIELDS
        .iter()
        .filter_map(|f| equate(c, &f.equate_name()).ok().map(|v| (f.key, v)))
        .collect()
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
    /// ⚑ **The cursor's address in `s4.debug.lst`**, read 2026-09-19 off the build of 2026-09-18 19:26
    /// (` Debug_Lab_Index : FFFFF00D C |`).
    ///
    /// It is a fixture's number and nothing in the shipped path holds it: [`forbidden`] is keyed on what
    /// the loaded listing answers ([`Cursor`]). It is here so the fake listing says something a real
    /// listing says, and `the_address_route_follows_the_listing_rather_than_any_number_in_this_file`
    /// deliberately uses a DIFFERENT address, so no green anywhere depends on this value being the live one.
    const S4_LAB_INDEX: u32 = 0xFFFF_F00D;

    /// ⚑ **What [`NOTE`] recorded and what this file used to ship as `LAB_INDEX_ADDR`** — kept as a
    /// witness, in the [`SCRATCH_NOTED_ADDR`] sense, and **never as a claim about the machine**.
    ///
    /// Its whole job is to be the number the guard must NOT be keyed on. aeon swept its RAM table at
    /// `61918621` and the cursor had moved `+$200` to [`S4_LAB_INDEX`] while six siblings slid `+$20`.
    /// `$FFFFEE0D` is now 13 bytes inside `Player_Pos_Ring` (`$FFFFEE00` in the same listing), so a guard
    /// still keyed here does not merely miss the cursor: it refuses somebody else's cell **in the cursor's
    /// name**, which is a refusal that misidentifies what it caught.
    const NOTED_STALE_LAB_INDEX: u32 = 0xFFFF_EE0D;

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
        /// ⚑ **A bus that will not answer the ADDRESS direction of `lookup_symbol`.** It exists so
        /// [`label_at`]'s `None` arm is reachable at all: with a working bus it cannot be, because the
        /// gate only ever probes at or past a symbol the same listing just resolved. A branch that no
        /// fixture can reach is a branch nobody has read.
        refuse_addr_lookup: bool,
        /// ⚑ **The `Equate Table`'s own namespace, kept SEPARATE from `listing`** exactly as the bus keeps
        /// it (§11.36 option A): an equate is a value, never an address, and folding the two here would let
        /// a test pass on a door the real server does not have.
        equates: Vec<(&'static str, u32)>,
        /// How many whole frames `emulator/run_frames` was asked for, summed.
        frames: u64,
    }

    impl Fake {
        fn new(listing: Vec<(&'static str, u32, u32)>) -> Self {
            Fake {
                listing,
                calls: Vec::new(),
                reads: Vec::new(),
                refuse_write: None,
                refuse_addr_lookup: false,
                equates: Vec::new(),
                frames: 0,
            }
        }

        /// Every symbol this module's three channels touch. **The two debug-only ones are present here**;
        /// the tests that are about their absence take them out rather than the other tests inventing
        /// them.
        ///
        /// ⚑ **THE RAM ROWS AND THEIR NEIGHBOURS ARE `s4.debug.lst` AS BUILT 2026-09-18 19:26**, read at
        /// this seat on 2026-09-19. Until this parcel two of them were the *note's* numbers instead —
        /// `Raster_Program` at `$FFFF8BD6` and `BgAnim_Table_Ptr` at `$FFFFE91A`, exactly what
        /// [`Channel::noted_addr`] still carries — and that is **how a real defect sat under a green
        /// suite**: the fixture agreed with the note, the shipped `drift` refusal compared the two, and
        /// every row over the raster and bands channels passed while both channels refused every gesture
        /// on a real build. A fixture that shares the defect's source cannot witness the defect.
        ///
        /// So the fixture now says what the listing says and the note keeps saying what the note said,
        /// which is the shape reality has. Every green over these two channels is therefore a green
        /// **across** a stated disagreement, which is the behaviour this parcel introduced.
        ///
        /// ⚑ **The four neighbour rows are the fixture's LAYOUT and not decoration.** `Raster_Line`,
        /// `Parallax_Vscroll_Column_Buf`, `Waterline_Art_Row` and `Static_Pal_Line0` are the symbols that
        /// actually follow these cells in that listing, and [`layout`] reads exactly that — where the
        /// next symbol starts — to decide whether a cell stays inside the symbol it names. Without them
        /// the fake listing would describe cells of unbounded width and the gate would have nothing to
        /// measure. `Static_Pal_Line0` is the load-bearing one: it is what makes `BgAnim_LastStep`
        /// **eight bytes**, which is the single transcribed layout fact in any of the three write-sets.
        ///
        /// The ROM addresses are the listing's too, and nothing here depends on them; they slide on
        /// every build.
        fn full() -> Self {
            let mut f = Fake::new(vec![
                ("Parallax_Current_Config", 0xFF_88EC, 0xFFFF_88EC),
                ("Parallax_Target_Config", 0xFF_88F0, 0xFFFF_88F0),
                ("Parallax_Transition_Frames", 0xFF_88F4, 0xFFFF_88F4),
                ("Parallax_Snap_Pending", 0xFF_88F5, 0xFFFF_88F5),
                // The neighbour that bounds `Parallax_Snap_Pending`.
                ("Parallax_Vscroll_Column_Buf", 0xFF_88F8, 0xFFFF_88F8),
                ("ParallaxConfig_Haze", 0x01_2C6C, 0x0001_2C6C),
                ("ParallaxConfig_OJZ_Default", 0x01_267A, 0x0001_267A),
                ("Raster_Program", 0xFF_8BF6, 0xFFFF_8BF6),
                ("Raster_Pending", 0xFF_8BFE, 0xFFFF_8BFE),
                // The neighbour that bounds `Raster_Pending` at four bytes.
                ("Raster_Line", 0xFF_8C02, 0xFFFF_8C02),
                ("Raster_Program_None", 0x00_881E, 0x0000_881E),
                ("EditorRaster_OJZ_Act1_ramp_probe", 0x01_4652, 0x0001_4652),
                ("BgAnim_Table_Ptr", 0xFF_E93A, 0xFFFF_E93A),
                // The neighbour that bounds `BgAnim_Table_Ptr` at four bytes.
                ("Waterline_Art_Row", 0xFF_E93E, 0xFFFF_E93E),
                ("BgAnim_LastStep", 0xFF_8F26, 0xFFFF_8F26),
                // ⚑ The neighbour that makes `BgAnim_LastStep` eight bytes, which is what the write-set's
                // `disp: 4` claims and what `layout`'s coverage gate pins.
                ("Static_Pal_Line0", 0xFF_8F2E, 0xFFFF_8F2E),
                ("BgAnim_Table", 0x02_8BD4, 0x0002_8BD4),
                ("Debug_Lab_Index", 0xFF_F00D, S4_LAB_INDEX),
            ]);
            // The one bg_anim layout equate the listing publishes, at the value it publishes.
            // ⚑ `with_equates` REPLACES this, which is what the parallax-hook rows want; a bands row
            // that needs it back says so.
            f.equates = vec![(BGANIM_MAX_BANDS_EQU, 4)];
            f
        }

        /// The equate rows the parallax scratch's layout is read out of. Values are the ones
        /// `s4.debug.lst` actually publishes, so a transcribed offset and a resolved one are only equal
        /// here because the listing says so.
        fn with_equates(mut self, rows: Vec<(&'static str, u32)>) -> Self {
            self.equates = rows;
            self
        }

        fn without_equate(mut self, name: &str) -> Self {
            self.equates.retain(|(n, _)| *n != name);
            self
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
                    if self.refuse_addr_lookup && params["addr"].is_string() {
                        return Err(Refusal::local("the bus did not answer".to_string()));
                    }
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
                // ⚑ Name only, and `-32013` with `data.missing` for an absent one — the two codes the
                // contract test pins, so this fake cannot make a refusal shape up.
                "emulator/lookup_equate" => {
                    let name = params["name"].as_str().unwrap_or_default();
                    match self.equates.iter().find(|(n, _)| *n == name) {
                        Some((n, v)) => Ok(json!({"name": n, "value": v})),
                        None => Err(Refusal {
                            code: Some(-32013),
                            reason: Some("equateNotPublished".to_string()),
                            message: format!("this listing does not publish `{name}`"),
                            remedy: None,
                        }),
                    }
                }
                "emulator/run_frames" => {
                    self.frames += params["frames"].as_u64().unwrap_or(0);
                    Ok(json!({"frames": params["frames"]}))
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
    /// would poke the cursor's cell through a symbol called something else.
    ///
    /// The control is the last assertion: a real selector at a real address passes, so the two above are
    /// refusing this address rather than refusing everything.
    #[test]
    fn the_lab_index_is_refused_by_name_and_by_address_independently() {
        let here = Cursor {
            raw: Some(S4_LAB_INDEX),
        };
        let by_name = forbidden(here, LAB_INDEX_SYMBOL, 0x00FF_0000)
            .expect("the cursor's NAME must be refused even at an unrelated address");
        assert!(
            by_name.message.contains("named as the write target"),
            "the refusal must say which route it caught: {}",
            by_name.message
        );

        let by_addr = forbidden(here, "Something_Else_Entirely", S4_LAB_INDEX)
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
            // ⚑ And it names the address it is talking about as the LISTING's, so a reader can check it.
            assert!(
                r.message.contains(&format!("{S4_LAB_INDEX:#010X}"))
                    && r.message.contains("the loaded listing puts it at"),
                "the refusal must say where the cursor was resolved to: {}",
                r.message
            );
        }

        // The control. Without it both rows above pass on a `forbidden` that refuses everything.
        assert_eq!(
            forbidden(here, PARALLAX.selector, 0xFF_88EC),
            None,
            "a real selector at a real address must pass, or the two rows above witness nothing"
        );
    }

    /// ⚑ **THE ADDRESS ROUTE FOLLOWS THE LISTING, NOT ANY NUMBER IN THIS FILE.**
    ///
    /// # This is the regression gate for the defect the resolve-instead-of-transcribe change fixes
    ///
    /// The guard shipped with a transcribed `LAB_INDEX_ADDR = $FFFFEE0D`, and aeon's sweep (`61918621`)
    /// moved the cursor to [`S4_LAB_INDEX`]. A transcribed guard then fails **both ways at once** — the
    /// real cursor is no longer refused, and [`NOTED_STALE_LAB_INDEX`], which is now inside
    /// `Player_Pos_Ring`, is refused in the cursor's name.
    ///
    /// ⚑ **The listing address used here is FICTIONAL and appears nowhere else in this crate**, which is
    /// the whole design of the row. A gate that used [`S4_LAB_INDEX`] would still pass if somebody
    /// re-keyed the guard on a fresh literal — it would be testing a coincidence. This one can only pass
    /// if the guard reads the address it was handed, so re-transcribing ANY constant fails it.
    #[test]
    fn the_address_route_follows_the_listing_rather_than_any_number_in_this_file() {
        // A number no listing on this box carries and no constant in this crate holds.
        const ELSEWHERE: u32 = 0xFFFF_BEEF;
        let moved = Cursor {
            raw: Some(ELSEWHERE),
        };

        let caught = forbidden(moved, "Some_Other_Cell", ELSEWHERE)
            .expect("the address route must fire wherever THIS listing puts the cursor");
        assert_eq!(caught.reason.as_deref(), Some("labIndexIsNotASelector"));
        assert!(
            caught.message.contains("resolves to that address"),
            "the ADDRESS route must be the one that caught it: {}",
            caught.message
        );

        // ⚑ And the two literals a transcribing guard would have used must now pass. Either one refusing
        // means the address route is keyed on a number rather than on the listing.
        for (what, addr) in [
            ("the note's stale address", NOTED_STALE_LAB_INDEX),
            ("s4.debug.lst's current one", S4_LAB_INDEX),
        ] {
            assert_eq!(
                forbidden(moved, "Some_Other_Cell", addr),
                None,
                "{what} ({addr:#010X}) was refused on a listing that puts the cursor at                  {ELSEWHERE:#010X}. The address route is keyed on a TRANSCRIBED constant again, which is                  the defect this row exists for: it refuses a cell that is not the cursor, in the                  cursor's name, and misses the one that is"
            );
        }
    }

    /// ⚑ **A LISTING WITHOUT THE CURSOR KEEPS THE NAME ROUTE AND SAYS THE ADDRESS ROUTE DID NOT RUN.**
    ///
    /// Measured 2026-09-19: `s4.debug.lst` carries `Debug_Lab_Index` and `s4.lst`, `demo.lst` and
    /// `demo.debug.lst` do not — so this is the common shape, not an edge. Three things are pinned, and
    /// the third is the one that makes this honest rather than merely quiet:
    ///
    /// 1. the name route still refuses;
    /// 2. the address route refuses **nothing**, including the addresses the old constant held — an
    ///    unresolvable cursor must not be turned into a refusal of some other cell;
    /// 3. the refusal **says** only the name route ran. A guard that silently checked half of what it
    ///    advertises would leave a reader believing both routes were live.
    #[test]
    fn an_unresolvable_cursor_keeps_the_name_route_and_says_the_address_route_did_not_run() {
        let absent = Cursor { raw: None };

        let by_name = forbidden(absent, LAB_INDEX_SYMBOL, 0x00FF_0000)
            .expect("the NAME route needs no listing and must still refuse");
        assert_eq!(by_name.reason.as_deref(), Some("labIndexIsNotASelector"));
        assert!(
            by_name.message.contains("ONLY THE NAME ROUTE RAN")
                && by_name.message.contains("was not run"),
            "the refusal must say how much of the guard ran: {}",
            by_name.message
        );

        for (what, addr) in [
            ("the note's stale address", NOTED_STALE_LAB_INDEX),
            ("s4.debug.lst's current one", S4_LAB_INDEX),
            ("the selector's", 0xFFFF_88EC),
        ] {
            assert_eq!(
                forbidden(absent, "Some_Other_Cell", addr),
                None,
                "{what} ({addr:#010X}) was refused on a listing that does not carry the cursor at all.                  With no resolved address there is nothing for the address route to compare against, and                  inventing one refuses a cell in the cursor's name"
            );
        }
    }

    /// ⚑ **An absent cursor does NOT refuse the feature**, and a broken bus is NOT reported as an absence.
    ///
    /// The two halves are one decision seen from both sides. A listing without `Debug_Lab_Index` is an
    /// ordinary release or no-lab build, so [`cursor`] answers `None` and the gesture proceeds with the
    /// name route — the drift-refusal trap this lane avoided for [`SCRATCH_NOTED_ADDR`], where a gate's red
    /// would have been about the layout rather than about the subject. But *no listing loaded at all* is
    /// not a statement about this symbol, and swallowing it would let a dead bus read as a build with no
    /// lab, so it is propagated.
    #[test]
    fn a_missing_cursor_symbol_is_not_a_refusal_and_a_dead_bus_is_not_a_missing_cursor() {
        // 1. The name is absent. The cursor resolves to nothing and the GESTURE STILL RUNS.
        let mut f = Fake::full().without(LAB_INDEX_SYMBOL);
        assert_eq!(
            cursor(&mut f).expect("an absent cursor is not a refusal"),
            Cursor { raw: None }
        );
        let w = point_at(&mut f, &PARALLAX, "ParallaxConfig_Haze")
            .expect("a listing without the cursor must still be able to select a scene");
        assert_eq!(w.cells.len(), PARALLAX.writes.len());

        // The control: the same fixture WITH the name resolves it, so the row above witnesses the absence
        // rather than a `cursor` that answers `None` for everything.
        let mut g = Fake::full();
        assert_eq!(
            cursor(&mut g).expect("the full fixture carries the cursor"),
            Cursor {
                raw: Some(S4_LAB_INDEX)
            },
            "`cursor` must read the listing's address, not a constant"
        );

        // 2. A bus that answers nothing at all is propagated, not converted into `raw: None`.
        struct Dead;
        impl Caller for Dead {
            fn call(&mut self, _m: &str, _p: Value) -> Result<Value, Refusal> {
                Err(Refusal {
                    code: Some(-32012),
                    reason: Some("noListing".to_string()),
                    message: "no listing is loaded".to_string(),
                    remedy: None,
                })
            }
            fn address_of(&mut self, _s: &str) -> Option<u32> {
                None
            }
        }
        let e = cursor(&mut Dead).expect_err("a bus with no listing is not a build without a lab");
        assert_eq!(e.reason.as_deref(), Some("noListing"));
    }

    /// ⚑ **The ADDRESS route fires on a symbol the bus resolved, not only on a value handed in by hand.**
    ///
    /// # This row exists because the guard it checks was dead, and its own sibling could not tell
    ///
    /// `the_lab_index_is_refused_by_name_and_by_address_independently` calls [`forbidden`] directly and
    /// passes. For a while the shipped path handed it the **24-bit door address** instead, and `$FFF00D`
    /// never equals `$FFFFF00D`, so the address branch could not fire on any real input while that sibling
    /// went on being green. The two spellings are the whole bug, so this row goes through `run` with a real
    /// listing and two real resolves — the cell's and the cursor's — which is the only arrangement that can
    /// see it.
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
            covers: &[],
            prefix: "ParallaxConfig_",
            debug_only: false,
            off: Off::No("n/a"),
            zero: "n/a",
            subject: "n/a",
        };

        let mut f = Fake::full();
        f.listing.push(("Some_Other_Cell", 0xFF_F00D, S4_LAB_INDEX));
        // ⚑ The cursor comes out of the same fixture listing the cell does, resolved here exactly as the
        // shipped gesture resolves it. Nothing in this row hands the guard a number from this file.
        let cur = cursor(&mut f).expect("the full fixture carries the cursor");
        let e = run(&mut f, cur, &TRAP, "whatever", 0, Vec::new(), None)
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
            forbidden(cur, "Some_Other_Cell", 0xFF_F00D),
            None,
            "the 24-bit door form cannot match the resolved `rawAddr`, which is why `run` resolves and \
             passes the 32-bit spelling. If this ever starts refusing, `resolve` has changed which \
             spelling it reports and the doc on `forbidden` is stale"
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
                let cur = cursor(&mut f).expect("the fake listing carries the cursor");
                assert_eq!(
                    forbidden(cur, w.symbol, raw.wrapping_add(w.disp)),
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
    // ⚑ Drift: the disagreement is a WITNESS, and the refusal is on the LAYOUT
    // ---------------------------------------------------------------------------------------------

    /// ⚑ **A MOVED SELECTOR DOES NOT REFUSE, AND THE MOVE IS STATED. This is the parcel's subject.**
    ///
    /// # The defect this row is the regression gate for
    ///
    /// `drift` refused whenever the resolved address differed from [`Channel::noted_addr`]. On
    /// `s4.debug.lst` as built 2026-09-18 that was **two of the three channels**, so the raster and the
    /// bands channel refused every gesture on a build with nothing wrong with it — and every test was
    /// green because `Fake::full` carried the note's numbers too.
    ///
    /// ⚑ **The listing this row hands the panel is FICTIONAL and appears nowhere else in this crate.**
    /// That is the whole design, borrowed from
    /// `the_address_route_follows_the_listing_rather_than_any_number_in_this_file`: a row written
    /// against `s4.debug.lst`'s real addresses would still pass if somebody "fixed" this by
    /// re-transcribing `noted_addr` to what the listing says today, which is the explicit thing the
    /// ruling rejects — it repairs today and reproduces the class. Keyed on a third address, the only
    /// way to pass is to **resolve**.
    #[test]
    fn a_moved_selector_is_stated_and_not_refused_whatever_the_two_numbers_are() {
        for c in CHANNELS {
            // A number no listing on this box carries and no constant in this crate holds.
            let elsewhere = 0xFFFF_BE00 | u32::from(c.key.len() as u8);
            assert_ne!(elsewhere, c.noted_addr);

            assert_eq!(
                c.witness(c.noted_addr),
                None,
                "{}: agreement is not a finding and must say nothing",
                c.key
            );

            let d = c
                .witness(elsewhere)
                .unwrap_or_else(|| panic!("{}: a disagreement must be witnessed", c.key));
            assert_eq!(d.resolved, elsewhere, "{}: the LISTING's number", c.key);
            assert_eq!(d.noted, c.noted_addr, "{}: the NOTE's number", c.key);

            // Both numbers in the sentence, or a reader has nothing to check.
            let line = d.line();
            assert!(
                line.contains(&format!("{elsewhere:#010X}"))
                    && line.contains(&format!("{:#010X}", c.noted_addr)),
                "{}: both addresses must be stated: {line}",
                c.key
            );
            // ⚑ And it must say which side won, because "two numbers differ" without that is the old
            // panel's confusion handed to the reader instead of its answer.
            assert!(
                line.contains("not blocking") && line.contains("LISTING"),
                "{}: the witness must say it did not block and which address was used: {line}",
                c.key
            );
        }
    }

    /// ⚑ **A GESTURE RUNS ACROSS A DISAGREEMENT, AND THE READOUT SAYS SO.**
    ///
    /// The end-to-end half of the row above, through `point_at` with a real write-set. The fixture's
    /// listing is moved wholesale to addresses **neither** [`NOTE`] nor `s4.debug.lst` carries, so:
    ///
    /// * a panel keyed on the note's numbers refuses (the shipped defect);
    /// * a panel keyed on freshly transcribed current numbers refuses too (the tempting wrong repair);
    /// * only a panel that resolves writes anything.
    ///
    /// ⚑ And the **witness must survive**: this row fails if the cells are written and the disagreement
    /// is swallowed, which is the exact shape of the fix that deletes `noted_addr` and passes
    /// everything else.
    #[test]
    fn a_disagreement_lets_the_gesture_through_and_the_readout_states_it() {
        let mut f = Fake::full();
        // Slide the whole RAM map by a displacement no build on this box has, layout untouched — which
        // is precisely what aeon's own sweep did (six symbols `+$20`, no width changed).
        const SLIDE: u32 = 0x0000_0700;
        for e in f.listing.iter_mut() {
            if e.1 >= 0xFF_0000 {
                e.1 += SLIDE;
                e.2 += SLIDE;
            }
        }

        let w = point_at(&mut f, &PARALLAX, "ParallaxConfig_Haze").expect(
            "a slid RAM map is a healthy build: every cell is addressed by NAME and the server \
             resolves it, so the gesture must go through",
        );
        assert_eq!(
            w.cells.len(),
            PARALLAX.writes.len(),
            "every cell of the write-set must have been written"
        );

        let d = w
            .drift
            .as_ref()
            .expect("the note and this listing disagree, and the witness must survive the gesture");
        assert_eq!(d.resolved, 0xFFFF_88EC + SLIDE);
        assert_eq!(d.noted, PARALLAX.noted_addr);
        // ⚑ Where a user sees it: on the readout's own headline, not in a log.
        let head = w.line();
        assert!(
            head.contains("DRIFT") && head.contains(&format!("{:#010X}", PARALLAX.noted_addr)),
            "the gesture's headline must carry the witness: {head}"
        );
        // And it names what the note's old address became in THIS listing, which is the lesson the
        // cursor guard was repaired for: a stale address is somebody else's cell, not a hole.
        assert!(
            d.now.is_some(),
            "the witness must say what the note's address is now: {d:?}"
        );

        // The control. Without the slide there is no disagreement and nothing is said, so the row
        // above witnesses the statement rather than a sentence that is always printed.
        let mut f = Fake::full();
        let w = point_at(&mut f, &PARALLAX, "ParallaxConfig_Haze").expect("the unmoved fixture");
        assert_eq!(
            w.drift, None,
            "the parallax note still agrees with the listing, so nothing is owed"
        );
        assert!(!w.line().contains("DRIFT"), "{}", w.line());
    }

    /// ⚑ **THE TWO CHANNELS THE SHIPPED REFUSAL BLANKED CAN BE SELECTED AND READ AGAIN.**
    ///
    /// Measured at this seat 2026-09-19: `Raster_Program` is `$FFFF8BF6` and `BgAnim_Table_Ptr` is
    /// `$FFFFE93A` in `s4.debug.lst`, against `$FFFF8BD6` and `$FFFFE91A` in [`NOTE`]. `Fake::full`
    /// now carries the listing's, so this row runs across a real, live disagreement rather than a
    /// synthetic one — and it would have been red before this parcel.
    ///
    /// The expectation is derived from [`Channel::noted_addr`] and the fixture, not typed: the row
    /// asserts *the channels whose note disagrees with the listing still work*, so it keeps its meaning
    /// if either number changes again.
    #[test]
    fn the_channels_whose_note_is_stale_still_select_and_still_read() {
        let mut disagreeing = 0;
        for c in CHANNELS {
            let mut f = Fake::full();
            let (_, raw) = resolve(&mut f, c.selector).expect("the fixture carries every selector");
            if c.witness(raw).is_none() {
                continue;
            }
            disagreeing += 1;

            let target = match c.key {
                "raster" => "EditorRaster_OJZ_Act1_ramp_probe",
                _ => "BgAnim_Table",
            };
            let w = point_at(&mut f, &c, target)
                .unwrap_or_else(|e| panic!("{} refused a healthy build: {}", c.key, e.message));
            assert_eq!(w.cells.len(), c.writes.len(), "{}", c.key);
            assert!(w.drift.is_some(), "{}: the witness must survive", c.key);

            // The read path too: it carried the identical refusal and blanked the readback.
            let mut f = Fake::full().serving(c.selector, "00000000");
            let l = live(&mut f, &c)
                .unwrap_or_else(|e| panic!("{} could not be read back: {}", c.key, e.message));
            assert!(
                l.line(&c).contains("DRIFT"),
                "{}: the readback must state the disagreement: {}",
                c.key,
                l.line(&c)
            );
        }
        // ⚑ Loud on unmeasurable. If the fixture is ever brought into agreement with the note, this row
        // stops testing anything and must say so rather than pass on an empty loop.
        assert_eq!(
            disagreeing, 2,
            "this row is about the channels whose note is stale; `Fake::full` and `noted_addr` must \
             still disagree on exactly the two measured on 2026-09-19, or the row witnesses nothing"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // ⚑ The layout gate: what a refusal is allowed to stand on
    // ---------------------------------------------------------------------------------------------

    /// ⚑ **THE GATE IS SILENT ON A UNIFORM SLIDE AND LOUD ON A STRUCT THAT CHANGED SHAPE.**
    ///
    /// The two halves are the whole argument for moving the refusal off the address. aeon's `61918621`
    /// slid six symbols by `+$20` and changed no width; a gate that fires on that refuses healthy
    /// builds, which is what the old one did.
    #[test]
    fn the_layout_gate_ignores_a_slide_and_catches_a_struct_that_changed_shape() {
        // 1. A uniform slide. Every address differs from both the note's and the listing's, and the
        //    gate must say nothing, because every comparison it makes is between two addresses from
        //    the SAME listing.
        for c in CHANNELS {
            let mut f = Fake::full();
            for e in f.listing.iter_mut() {
                if e.1 >= 0xFF_0000 {
                    e.1 += 0x0000_0700;
                    e.2 += 0x0000_0700;
                }
            }
            let said = layout(&mut f, &c)
                .unwrap_or_else(|e| panic!("{} refused a slid RAM map: {}", c.key, e.message));
            assert!(
                said.iter().all(|l| !l.starts_with('⚠')),
                "{}: a slid map is fully measurable: {said:?}",
                c.key
            );
        }

        // 2. The shape changed. `BgAnim_LastStep`'s neighbour moves in by four bytes, which is exactly
        //    an array that lost half its band slots — and it is the ONLY mutation that makes the
        //    write-set's transcribed `disp: 4` address somebody else's cell.
        let mut f = Fake::full();
        for e in f.listing.iter_mut() {
            if e.0 == "Static_Pal_Line0" {
                e.1 -= 4;
                e.2 -= 4;
            }
        }
        let e = layout(&mut f, &BANDS).expect_err("the poison's second longword now lands outside");
        assert_eq!(e.reason.as_deref(), Some("cellLeavesItsSymbol"));
        assert!(
            e.message.contains("Static_Pal_Line0") && e.message.contains("`BgAnim_LastStep`+4"),
            "the refusal must name the cell and what it would have written into: {}",
            e.message
        );
        // ⚑ And it must say it is a LAYOUT finding, so a reader does not go looking for a moved ROM.
        assert!(e.message.contains("LAYOUT finding"), "{}", e.message);

        // The gesture stops before the first cell, not partway through it.
        let mut f = Fake::full();
        for e in f.listing.iter_mut() {
            if e.0 == "Static_Pal_Line0" {
                e.1 -= 4;
                e.2 -= 4;
            }
        }
        point_at(&mut f, &BANDS, "BgAnim_Table").expect_err("refused");
        assert!(f.writes().is_empty(), "wrote {:?} anyway", f.writes());
    }

    /// ⚑ **AN ARRAY THAT OUTGREW ITS POISON IS CAUGHT, AND THAT IS THE CHECK NO PER-CELL TEST CAN MAKE.**
    ///
    /// The per-cell check above sees a struct that **shrank**. An array that **grew** — a fifth band
    /// index — leaves every cell inside its own symbol and is therefore invisible to it, while
    /// `bg_anim.emp`'s own header says the consequence in the imperative: the un-poisoned index takes
    /// the `.skip_band` arm forever and *"the new table would never paint"* on it.
    ///
    /// Both facts here come out of the fixture's listing and its equate table, never from a number in
    /// this row: the extent is pinned by where the next symbol starts, and the band count by the
    /// equate the listing publishes.
    #[test]
    fn an_array_that_outgrew_the_write_sets_poison_is_refused() {
        let mut f = Fake::full();
        // The array grows by four bytes — two more band indices at the listing's own two bytes each —
        // and its neighbour moves out of the way. Every cell still lands inside `BgAnim_LastStep`.
        for e in f.listing.iter_mut() {
            if e.0 == "Static_Pal_Line0" {
                e.1 += 4;
                e.2 += 4;
            }
        }
        f.equates = vec![(BGANIM_MAX_BANDS_EQU, 6)];

        let e = layout(&mut f, &BANDS).expect_err("the write-set no longer covers the array");
        assert_eq!(e.reason.as_deref(), Some("arrayOutgrewItsPoison"));
        assert!(
            e.message.contains("BgAnim_LastStep") && e.message.contains("8 bytes"),
            "the refusal must name the array and what the write-set actually covers: {}",
            e.message
        );
        // ⚑ It must state the one shape it cannot tell apart, rather than claim certainty.
        assert!(
            e.message.contains("LAST symbol"),
            "the gate must say what it is blind to: {}",
            e.message
        );

        // The control: the unmutated fixture passes the same gate and SAYS what it established, in the
        // units the listing publishes. Without this the row above passes on a gate that refuses always.
        let mut f = Fake::full();
        let said = layout(&mut f, &BANDS).expect("the measured listing must pass");
        assert!(
            said.iter()
                .any(|l| l.contains("BgAnim_LastStep") && l.contains("4 band index slots")),
            "the gate must report what it established, in the listing's own count: {said:?}"
        );
    }

    /// ⚑ **A MISSING COUNT EQUATE IS A STATED LIMIT, NEVER A SILENT PASS AND NEVER A REFUSAL.**
    ///
    /// Nothing is computed from the count — it turns a byte figure into the figure the engine reasons
    /// in — so a listing that does not publish it must not cost the gesture. But a gate that quietly
    /// checked less than it advertises leaves a reader believing the whole of it ran, which is the rule
    /// `Cursor::routes` was written for on the other guard.
    #[test]
    fn an_unmeasurable_partition_is_said_rather_than_passed_over() {
        let mut f = Fake::full();
        f.equates.clear();
        let said = layout(&mut f, &BANDS).expect("a missing equate must not refuse the gesture");
        let warned: Vec<&String> = said.iter().filter(|l| l.starts_with('⚠')).collect();
        assert_eq!(warned.len(), 1, "exactly one fact went unchecked: {said:?}");
        assert!(
            warned[0].contains(BGANIM_MAX_BANDS_EQU) && warned[0].contains("does not publish"),
            "the limit must name the equate it wanted: {}",
            warned[0]
        );

        // A count that does not divide is the same class and is also stated rather than refused.
        let mut f = Fake::full();
        f.equates = vec![(BGANIM_MAX_BANDS_EQU, 3)];
        let said = layout(&mut f, &BANDS).expect("a non-dividing count must not refuse either");
        assert!(
            said.iter()
                .any(|l| l.starts_with('⚠') && l.contains("does not divide")),
            "{said:?}"
        );
    }

    /// ⚑ **A PROBE THAT DID NOT ANSWER IS SAID, AND THE ACCOUNT STOPS CLAIMING WHAT IT DID NOT CHECK.**
    ///
    /// The first cut of this gate pushed its affirmative summary unconditionally, so a run in which no
    /// cell could be placed still ended *"and it is"* under a `⚠` line saying the check had not run.
    /// That is the silent pass the whole parcel is about, produced inside the gate written against it,
    /// and the method was tightened partway: the row is therefore retroactive rather than original.
    ///
    /// It also proves the arm is reachable. With a working bus it is not — the gate only probes at or
    /// past a symbol the same listing just resolved, so something always precedes it — which is exactly
    /// why the fixture needs a bus that refuses the address direction. A branch no fixture can reach is
    /// a branch nobody has read.
    #[test]
    fn a_probe_that_did_not_answer_is_stated_and_the_account_stops_claiming_the_check() {
        for c in CHANNELS {
            let mut f = Fake::full();
            f.refuse_addr_lookup = true;
            let said = layout(&mut f, &c).unwrap_or_else(|e| {
                panic!(
                    "{}: a bus that will not answer a probe is not evidence of a layout fault and \
                     must not refuse the gesture: {}",
                    c.key, e.message
                )
            });
            assert_eq!(
                said.iter().filter(|l| l.starts_with('⚠')).count(),
                c.writes.len() + 1 + 2 * c.covers.len(),
                "{}: every unplaced cell owes a line, plus the incomplete-account line, plus TWO per \
                 array: the extent that went unpinned and the partition that cannot stand on it: \
                 {said:?}",
                c.key
            );
            assert!(
                said.iter().any(|l| l.contains("account is INCOMPLETE")),
                "{}: {said:?}",
                c.key
            );
            // ⚑ And nothing may still assert the affirmative. This is the clause the first cut failed.
            assert!(
                !said.iter().any(|l| l.contains("and it is")),
                "{}: the gate claimed the check it just said it could not run: {said:?}",
                c.key
            );
        }
        // The control: with the bus answering, the same fixture produces no warning at all, so the row
        // above witnesses the refusing bus rather than a gate that always warns.
        for c in CHANNELS {
            let mut f = Fake::full();
            assert!(
                layout(&mut f, &c)
                    .expect("answers")
                    .iter()
                    .all(|l| !l.starts_with('⚠')),
                "{}",
                c.key
            );
        }
    }

    /// ⚑ **WHAT EACH CHANNEL'S GATE ACTUALLY TESTS, STATED PER CHANNEL.**
    ///
    /// The three are **not alike** and the difference is a measured finding rather than an oversight:
    ///
    /// * **parallax** and **raster** write only whole symbols at displacement 0, so their write-sets
    ///   carry no transcribed field offset for a layout gate to check. What is left is real but
    ///   narrow — each symbol is still at least as wide as the write — and the readout says so in those
    ///   words rather than implying the offsets were verified.
    /// * **bands** carries the one transcribed offset in the module (`BgAnim_LastStep+4`) and the one
    ///   array poisoned whole, so it is the only channel with both gates live.
    #[test]
    fn each_channels_gate_reports_what_it_could_check_and_the_three_differ() {
        for c in CHANNELS {
            let mut f = Fake::full();
            let said = layout(&mut f, &c).unwrap_or_else(|e| panic!("{}: {}", c.key, e.message));
            assert!(!said.is_empty(), "{}: every channel owes an account", c.key);

            let scalar_only = c.writes.iter().all(|w| w.disp == 0);
            assert_eq!(
                scalar_only,
                c.covers.is_empty(),
                "{}: a channel with an array to poison is exactly a channel with a transcribed \
                 displacement, in this module; if that stops being true this row needs rewriting \
                 rather than relaxing",
                c.key
            );
            if scalar_only {
                assert!(
                    said.iter()
                        .any(|l| l.contains("no transcribed field offset")),
                    "{}: a channel with nothing to check must say so: {said:?}",
                    c.key
                );
            } else {
                assert!(
                    said.iter().any(|l| l.contains("transcribed displacements")),
                    "{}: {said:?}",
                    c.key
                );
            }
        }
        // The shape this row is about, pinned so the sentences above cannot both be vacuous.
        assert_eq!(
            CHANNELS.iter().filter(|c| c.covers.is_empty()).count(),
            2,
            "two of the three channels have no array to poison, as measured"
        );
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
            drift: None,
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
            drift: None,
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

    /// ⚑ **A drifted selector STATES the drift on the readback, and answers it.**
    ///
    /// This row is the inverse of the one it replaces. The old shape refused the readback as well as
    /// the write, on the argument that a panel which will not write a cell it cannot place should not
    /// print a confident sentence about that cell's contents. **The premise was wrong in both halves**:
    /// the read goes to the address the LISTING resolves, which is the cell the engine reads, so the
    /// sentence is about the right four bytes — and on `s4.debug.lst` the refusal blanked the raster
    /// and bands readbacks on a build with nothing wrong with it.
    ///
    /// What the two paths share now is the statement. The readback answers **and** says the note
    /// disagrees, which is the only shape in which the reader is told both true things.
    #[test]
    fn a_drifted_selector_states_the_drift_on_the_readback_rather_than_blanking_it() {
        let mut f = Fake::full().serving(RASTER.selector, "0x0000881E");
        for e in f.listing.iter_mut() {
            if e.0 == RASTER.selector {
                e.1 = 0xFF_9000;
                e.2 = 0xFFFF_9000;
            }
        }
        let l =
            live(&mut f, &RASTER).expect("a moved cell is still readable at the resolved address");
        assert_eq!(
            l.value, 0x0000_881E,
            "the readback must still be the cell's"
        );
        let d = l.drift.as_ref().expect("and the move must be stated");
        assert_eq!(d.resolved, 0xFFFF_9000);
        assert_eq!(d.noted, RASTER.noted_addr);
        assert!(l.line(&RASTER).contains("DRIFT"), "{}", l.line(&RASTER));

        // The control, and it is not the obvious one: the UNMUTATED fixture also drifts on this
        // channel, because `Raster_Program` really has moved. So the control that proves the witness
        // is conditional has to be a channel whose note still agrees — parallax.
        let mut f = Fake::full().serving(PARALLAX.selector, "0x00012C6C");
        let l = live(&mut f, &PARALLAX).expect("undrifted");
        assert_eq!(l.drift, None, "agreement is not a finding");
        assert!(
            !l.line(&PARALLAX).contains("DRIFT"),
            "{}",
            l.line(&PARALLAX)
        );
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

    // ---------------------------------------------------------------------------------------------
    // ⚑ The nudge surface. Every fixture's numbers are MEASURED off the listings on this box, never
    // chosen, so an assertion that passes here is an assertion about a real build.
    // ---------------------------------------------------------------------------------------------

    /// `s4.debug.lst`'s own rows, read 2026-09-19 from the build of 2026-09-18 19:26.
    ///
    /// ⚑ **`Parallax_Scratch_Config` is at `$FFFFEA46` here and [`SCRATCH_NOTED_ADDR`] says `$FFFFEA26`.**
    /// That is not a typo in either place: the symbol sits at the RAM tail inside a size-varying
    /// `@shape_divergent` group and has moved $20 since the note was written. The fixture carries the
    /// listing's number, because the listing is what a machine runs with.
    const S4_SCRATCH: u32 = 0xFFFF_EA46;
    const S4_SCRATCH_END: u32 = 0xFFFF_EC64;

    /// `demo.debug.lst`'s, read the same day. Its span is 190 bytes, so its band stride is **10** where
    /// s4's is 32 — the measurement that rules out transcribing a stride.
    const DEMO_SCRATCH: u32 = 0xFFFF_E550;
    const DEMO_SCRATCH_END: u32 = 0xFFFF_E60E;

    /// The equate rows `s4.debug.lst` publishes for the two structs, values included.
    fn scratch_equates() -> Vec<(&'static str, u32)> {
        vec![
            ("parallax_config_len", 0x1E),
            ("MAX_PARALLAX_BANDS", 0x10),
            ("parallax_config_pcfg_band_count", 0x00),
            ("parallax_config_pcfg_layer_mask", 0x02),
            ("parallax_config_pcfg_deform_speed_fg", 0x09),
            ("parallax_config_pcfg_deform_speed_bg", 0x0A),
            ("parallax_config_pcfg_bob", 0x1D),
            ("band_entry_band_factor_a_s1", 0x02),
            ("band_entry_band_factor_a_s2", 0x03),
            ("band_entry_band_factor_b_s1", 0x04),
            ("band_entry_band_factor_b_s2", 0x05),
            ("band_entry_band_factor_ops", 0x06),
            ("band_entry_band_phase_offset", 0x09),
        ]
    }

    /// A scratch buffer of `bands` records, with `pcfg_band_count` at offset 0 and every other byte a
    /// distinct value, so a wrongly computed offset reads a number no other offset holds.
    ///
    /// ⚑ **Distinct bytes rather than a pattern, deliberately.** A buffer of zeroes or of one repeated
    /// byte would let an offset that is wrong by any amount read the value the test expected, which is a
    /// fixture that cannot fail.
    fn scratch_bytes(bands: u8) -> String {
        let len = 0x1E + 32 * bands as usize;
        let mut hex = String::from("0x");
        for i in 0..len {
            let b = if i == 0 { bands } else { (i as u8) ^ 0x5A };
            hex.push_str(&format!("{b:02X}"));
        }
        hex
    }

    /// The whole s4-shaped surface: the selector, the three scratch symbols, the proc, the equates, and a
    /// machine whose scratch IS installed with `bands` bands and a cleared arm byte.
    fn armed(bands: u8) -> Fake {
        Fake::new(vec![
            ("Parallax_Current_Config", 0xFF_88EC, 0xFFFF_88EC),
            ("Parallax_Target_Config", 0xFF_88F0, 0xFFFF_88F0),
            ("Parallax_Transition_Frames", 0xFF_88F4, 0xFFFF_88F4),
            ("Parallax_Snap_Pending", 0xFF_88F5, 0xFFFF_88F5),
            (
                "Parallax_Scratch_Config",
                S4_SCRATCH & 0x00FF_FFFF,
                S4_SCRATCH,
            ),
            (
                "Parallax_Scratch_Config_End",
                S4_SCRATCH_END & 0x00FF_FFFF,
                S4_SCRATCH_END,
            ),
            (
                "Parallax_Scratch_Arm",
                S4_SCRATCH_END & 0x00FF_FFFF,
                S4_SCRATCH_END,
            ),
            ("Parallax_InstallScratch", 0x01_2345, 0x0001_2345),
            ("Debug_Lab_Index", 0xFF_F00D, S4_LAB_INDEX),
        ])
        .with_equates(scratch_equates())
        // installed: the selector holds the scratch's own raw address
        .serving("Parallax_Current_Config", "0xFFFFEA46")
        // the request byte, cleared by the engine as it serviced it
        .serving("Parallax_Scratch_Arm", "0x00")
        .serving("Parallax_Scratch_Config", &scratch_bytes(bands))
    }

    /// ⚑ **THE TRAP FROM §5.3, IN ITS SECOND INCARNATION: the proc's name ships in a release listing.**
    ///
    /// `NOTE` §6.6 measured it — `Parallax_InstallScratch` appears in the RELEASE listing with an address
    /// (its body is DEBUG-gated, so the empty label collapses onto its neighbour's) while
    /// `Parallax_Scratch_Config` and `Parallax_Scratch_Arm` do not. A gate keyed on the proc would offer
    /// knobs on a release build and write into whatever occupies the buffer's old address, with no fault
    /// to show for it: the bands-off trap exactly, one channel over.
    ///
    /// So this fixture is a **release** listing: the proc present, the two RAM symbols absent.
    #[test]
    fn the_gate_ignores_the_proc_because_its_name_ships_in_a_release_listing() {
        let mut f = armed(2).without(SCRATCH).without(SCRATCH_ARM);
        let e = hook(&mut f).expect_err("a release listing must refuse, proc or no proc");
        assert_eq!(e.reason.as_deref(), Some("noScratchInThisBuild"));
        assert!(
            !f.calls.iter().any(|(m, p)| m == "emulator/lookup_symbol"
                && p["name"] == json!(SCRATCH_PROC)),
            "the gate resolved `{SCRATCH_PROC}`, whose name is in a RELEASE listing. Resolving it at all \
             is the defect, because whatever is done with the answer, the answer is yes on a build with \
             no buffer: {:?}",
            f.calls
        );
    }

    /// ⚑ **THE DESTINATION IS RESOLVED FIRST**, which is what makes the proc's release visibility harmless
    /// rather than dangerous — §5.3's own ordering, and the property `turn_off` already holds.
    ///
    /// The assertion is on the FIRST symbol lookup, not on the set of them: a gate that asked about the arm
    /// cell first would refuse for the right reason half the time and, on a build carrying one and not the
    /// other, name the wrong missing symbol in the remedy.
    #[test]
    fn the_destination_is_resolved_before_the_arm_cell() {
        let mut f = armed(2).without(SCRATCH).without(SCRATCH_ARM);
        let _ = hook(&mut f);
        let first = f
            .calls
            .iter()
            .find(|(m, _)| m == "emulator/lookup_symbol")
            .map(|(_, p)| p["name"].as_str().unwrap_or_default().to_string());
        assert_eq!(
            first.as_deref(),
            Some(SCRATCH),
            "the first thing the gate asks the listing must be the DESTINATION: {:?}",
            f.calls
        );
    }

    /// ⚑ **TWO conditions, and a build with one of them is refused.**
    ///
    /// A name resolving is not storage existing, and one name resolving is not two. The two symbols are in
    /// the same `if DEBUG == 1 @shape_divergent` block today, so this is a build that cannot exist by
    /// construction — which is the point: the gate is two checks because the bands-off precedent proves
    /// that reasoning from "they must go together" is how a panel comes to write a real address into a cell
    /// that is not the destination.
    #[test]
    fn a_build_with_the_buffer_and_no_arm_cell_is_refused() {
        let mut f = armed(2).without(SCRATCH_ARM);
        let e = hook(&mut f).expect_err("the buffer alone is not the hook");
        assert_eq!(e.reason.as_deref(), Some("noScratchInThisBuild"));
    }

    /// ⚑ **THE REFUSAL MUST READ AS *YOUR BUILD HAS NO SCRATCH*, NOT AS *THIS IS BROKEN*.**
    ///
    /// §5.3 paid for this wording once: the bands-off target landed at 16:48Z and the owner's window held a
    /// 14:53Z listing, so a correct refusal looked like a broken feature. Here a third door is open that
    /// was not open there — **a release ROM genuinely does not have this RAM** — so the line has to name
    /// the shape, not only the staleness.
    ///
    /// Each assertion is a question a person asks on reading it, and the last two are the ones that keep it
    /// from reading as a defect.
    #[test]
    fn the_no_scratch_refusal_names_the_shape_and_the_symbol_and_never_reads_as_broken() {
        let r = no_scratch();
        let m = r.message.clone();
        for want in [SCRATCH, SCRATCH_ARM, SCRATCH_PROC, "RELEASE", "DEBUG"] {
            assert!(m.contains(want), "the refusal must name {want:?}: {m}");
        }
        assert!(
            m.contains("not a fault") || m.contains("This is not a fault"),
            "it must say so in as many words, because the reader's first reading is that it is: {m}"
        );
        assert!(
            m.contains("Selecting a scene still works"),
            "it must say what DOES work here, or a person concludes the whole tab is dead: {m}"
        );
        for barred in ["broken", "failed", "error"] {
            assert!(
                !m.to_lowercase().contains(barred),
                "the refusal uses the word {barred:?}, which is the reading it exists to prevent: {m}"
            );
        }
        let remedy = r.remedy(None).unwrap_or_default();
        assert!(
            remedy.contains("debug") || remedy.contains("DEBUG") || remedy.contains("s4.debug"),
            "the remedy must name the shape that HAS the buffer: {remedy}"
        );
        assert!(
            remedy.contains(HOOK),
            "and the commit, so a person with a debug build can tell a stale listing from a missing \
             feature: {remedy}"
        );
    }

    /// ⚑ **THE BAND STRIDE IS DERIVED, AND A TRANSCRIBED 32 WOULD HAVE BEEN WRONG ON demo.**
    ///
    /// Both spans are measured: `s4.debug` gives `$FFFFEC64 - $FFFFEA46` = 542 → (542-30)/16 = **32**, and
    /// `demo.debug` gives `$FFFFE60E - $FFFFE550` = 190 → (190-30)/16 = **10**. `NOTE` §6.4 says 32 *"for
    /// this game"* and this is what that clause costs a panel that reads past it.
    ///
    /// ⚑ The two arms differ in **one** thing — the span — and share everything else, so a green here is
    /// about the derivation and not about two fixtures that happen to agree with themselves.
    #[test]
    fn the_band_stride_is_derived_from_the_span_and_is_32_on_s4_and_10_on_demo() {
        let mut f = armed(2);
        let h = hook(&mut f).expect("the s4-shaped listing");
        assert_eq!(
            (h.span, h.header_len, h.max_bands, h.stride),
            (542, 30, 16, 32)
        );

        let mut g = armed(2)
            .without(SCRATCH)
            .without(SCRATCH_END)
            .without(SCRATCH_ARM);
        g.listing.push((
            "Parallax_Scratch_Config",
            DEMO_SCRATCH & 0x00FF_FFFF,
            DEMO_SCRATCH,
        ));
        g.listing.push((
            "Parallax_Scratch_Config_End",
            DEMO_SCRATCH_END & 0x00FF_FFFF,
            DEMO_SCRATCH_END,
        ));
        g.listing.push((
            "Parallax_Scratch_Arm",
            DEMO_SCRATCH_END & 0x00FF_FFFF,
            DEMO_SCRATCH_END,
        ));
        let h = hook(&mut g).expect("the demo-shaped listing");
        assert_eq!(
            (h.span, h.stride),
            (190, 10),
            "demo's band record is 10 bytes, and a 32 written into this crate would address its band 1 \
             inside its band 3"
        );
    }

    /// ⚑ **A SPAN THAT DOES NOT FACTOR IS REFUSED, NEVER ROUNDED.**
    ///
    /// A rounded stride is worse than no stride: it is right for band 0 and wrong for every band after it,
    /// forever, with no fault — writes land in the middle of fields. So a non-exact division means the
    /// derivation's premise (one header, `MAX_PARALLAX_BANDS` equal records, nothing else in the span) does
    /// not hold for this build, and that is a refusal with the four numbers in it.
    #[test]
    fn a_span_that_does_not_factor_is_refused_rather_than_rounded() {
        let mut f = armed(2).without(SCRATCH_END).without(SCRATCH_ARM);
        // One byte longer, so (543 - 30) = 513 does not divide by 16.
        f.listing
            .push(("Parallax_Scratch_Config_End", 0xFF_EC65, 0xFFFF_EC65));
        f.listing
            .push(("Parallax_Scratch_Arm", 0xFF_EC65, 0xFFFF_EC65));
        let e = hook(&mut f).expect_err("543 - 30 = 513 does not divide by 16");
        assert_eq!(e.reason.as_deref(), Some("scratchDoesNotFactor"));
        for n in ["543", "30", "16"] {
            assert!(
                e.message.contains(n),
                "the refusal must carry the arithmetic so a reader can check it: {n} missing from {}",
                e.message
            );
        }
    }

    /// ⚑ **EVERY OFFERED FIELD'S OFFSET COMES OUT OF THE LISTING, NOT OUT OF THIS FILE.**
    ///
    /// The fixture publishes `band_entry_band_factor_a_s1` at **7**, which is not its real value (2). If
    /// this module transcribed the offset, the write's `disp` would be `30 + 32*1 + 2 = 64`; resolving it
    /// gives `30 + 32*1 + 7 = 69`. A green here is the write having gone where the LISTING said.
    ///
    /// ⚑ The value is deliberately one no other offered field holds, so a wrong lookup cannot land on it.
    #[test]
    fn every_offered_fields_offset_comes_from_an_equate_and_never_from_this_file() {
        let mut f = armed(2).without_equate("band_entry_band_factor_a_s1");
        f.equates.push(("band_entry_band_factor_a_s1", 7));
        let field = FIELDS
            .iter()
            .find(|f| f.key == "factor_a_s1")
            .expect("the field exists");
        nudge(&mut f, field, 1, 3).expect("band 1 is inside a 2-band config");
        assert_eq!(
            f.writes(),
            vec![(SCRATCH.to_string(), 69, 3, 1)],
            "the write must land at header(30) + stride(32)*band(1) + THE LISTING'S OFFSET(7) = 69, not \
             at the 64 a transcribed 2 would give"
        );
    }

    /// ⚑ **A FIELD WHOSE EQUATE THIS LISTING DOES NOT PUBLISH IS REFUSED, NEVER DEFAULTED TO ZERO.**
    ///
    /// Offset 0 is `pcfg_band_count`. A defaulted zero would therefore not merely miss the field, it would
    /// write the person's value into the band count — the one header byte whose wrong value makes the walk
    /// read bytes the install never wrote.
    #[test]
    fn a_field_whose_offset_equate_is_absent_is_refused_and_not_written_at_zero() {
        let mut f = armed(2).without_equate("parallax_config_pcfg_layer_mask");
        let field = FIELDS
            .iter()
            .find(|f| f.key == "layer_mask")
            .expect("the field exists");
        let e =
            nudge(&mut f, field, 0, 0x00F0).expect_err("an unpublished equate is not an offset");
        assert_eq!(
            e.code,
            Some(-32013),
            "the server's own code is carried up: {e:?}"
        );
        assert!(
            f.writes().is_empty(),
            "nothing may be written: {:?}",
            f.writes()
        );
    }

    /// ⚑ **THE INSTALL COMPARE IS MASKED ON BOTH SIDES**, which is correct for every spelling combination
    /// rather than for the pair that happened to be measured.
    ///
    /// `NOTE` §6.2 warns it explicitly — *"Mask before comparing; a raw compare is a false mismatch, and it
    /// was the first thing this lane's own probe got wrong"* — because `Parallax_Current_Config` stores the
    /// full sign-extended long while a listing may resolve the symbol to the 24-bit bus address.
    ///
    /// ⚑ **The question is the SPACE of values, not the values seen.** There are four combinations of
    /// spelling across the two sides; masking is right in all four and a raw compare is right in one, so
    /// both arms here are drawn from the space rather than from a measurement.
    #[test]
    fn the_install_compare_masks_both_sides_so_either_listing_spelling_matches() {
        for (current, scratch, what) in [
            (0xFFFF_EA46u32, 0xFFFF_EA46u32, "both raw"),
            (
                0xFFFF_EA46,
                0x00FF_EA46,
                "raw cell against a 24-bit listing",
            ),
            (
                0x00FF_EA46,
                0xFFFF_EA46,
                "24-bit cell against a raw listing",
            ),
            (0x00FF_EA46, 0x00FF_EA46, "both 24-bit"),
        ] {
            assert!(
                Installed {
                    current,
                    scratch,
                    arm: 0
                }
                .took(),
                "{what}: {current:#010X} and {scratch:#010X} are the same location and must compare equal"
            );
        }
        assert!(
            !Installed {
                current: 0x0001_2F08,
                scratch: 0xFFFF_EA46,
                arm: 0
            }
            .took(),
            "a ROM config is not the scratch, and masking must not make it one"
        );
    }

    /// ⚑ **THREE STATES OUT OF TWO FACTS, AND NO INVENTED STATUS BYTE.**
    ///
    /// `NOTE` §6.2 is explicit that the arm cell is a **request** byte — the engine clears it as it services
    /// it, whether the install took or was refused — so it cannot answer *did it work*. The compare answers
    /// that. The arm byte is read for one job only: separating *the engine refused* from *the engine never
    /// looked*, which the compare alone renders identical and which send a person to entirely different
    /// places.
    ///
    /// The `never serviced` arm is not hypothetical: `HOOK`'s own banner names `games/demo`, where
    /// `Parallax_Update` has no caller at all, and says the arm cell stays set for ever there and *"reads
    /// like a dirty refusal and is nothing of the kind"*.
    #[test]
    fn the_three_install_states_are_told_apart_by_the_arm_byte_and_named_differently() {
        let took = Installed {
            current: 0xFFFF_EA46,
            scratch: 0xFFFF_EA46,
            arm: 0,
        };
        let refused = Installed {
            current: 0x0001_2F08,
            scratch: 0xFFFF_EA46,
            arm: 0,
        };
        let unserviced = Installed {
            current: 0x0001_2F08,
            scratch: 0xFFFF_EA46,
            arm: 1,
        };
        assert!(took.took() && !refused.took() && !unserviced.took());
        assert!(
            took.line().contains("IS the current config"),
            "{}",
            took.line()
        );
        assert!(
            refused.line().contains("REFUSED") && refused.line().contains("pcfg_band_count"),
            "a serviced refusal must name what the engine refuses FOR, which is the only actionable part: \
             {}",
            refused.line()
        );
        assert!(
            unserviced.line().contains("NEVER SERVICED")
                && unserviced.line().contains("not a refusal"),
            "an unserviced arm must not read as a refusal — HOOK's banner says exactly that it does and \
             is not: {}",
            unserviced.line()
        );
        assert_ne!(
            refused.line(),
            unserviced.line(),
            "two states that send a person to different places must not share a sentence"
        );
    }

    /// ⚑ **A WRITE INTO A SCRATCH THE ENGINE IS NOT READING IS THE SILENT NO-OP THIS MODULE EXISTS
    /// AGAINST**, and it is refused with nothing written.
    ///
    /// It is not an exotic state. `NOTE` §6.6's consequence 1: **crossing a section boundary EVICTS the
    /// scratch**, because `Parallax_CheckBoundary` installs the new section's own ROM preset exactly as it
    /// always did. So a person who armed, walked right, and turned a knob would otherwise watch nothing
    /// happen — which is `NOTE` §0's whole subject.
    #[test]
    fn a_nudge_into_a_scratch_that_is_not_the_current_config_writes_nothing() {
        let mut f = armed(2);
        // The boundary crossing: the selector now holds the new section's ROM preset.
        f.reads.retain(|(k, _)| k != "Parallax_Current_Config");
        f.reads
            .push(("Parallax_Current_Config".into(), "0x00012F08".into()));
        let field = FIELDS.iter().find(|f| f.key == "factor_a_s1").unwrap();
        let e = nudge(&mut f, field, 0, 3).expect_err("an evicted scratch is not editable");
        assert_eq!(e.reason.as_deref(), Some("scratchNotInstalled"));
        assert!(
            f.writes().is_empty(),
            "nothing may be written: {:?}",
            f.writes()
        );
        let remedy = e.remedy(None).unwrap_or_default();
        assert!(
            remedy.contains("boundary") && remedy.contains("again"),
            "the remedy must name the ordinary cause and the fix, or the refusal reads as a defect: \
             {remedy}"
        );
    }

    /// ⚑ **THE BAND BOUND IS THE INSTALLED COUNT, NOT THE RESERVATION**, and the difference is bytes the
    /// install never wrote.
    ///
    /// `Parallax_InstallScratch` copies *"the header plus this config's OWN bands, not the ceiling"*, so a
    /// band between `pcfg_band_count` and `MAX_PARALLAX_BANDS` is inside the buffer and outside the scene:
    /// a write there edits uninitialised RAM that nothing reads. Bounding on `max_bands` would accept it.
    #[test]
    fn a_band_at_or_above_the_installed_count_is_refused_and_not_clamped() {
        let mut f = armed(2);
        let field = FIELDS.iter().find(|f| f.key == "factor_a_s1").unwrap();
        // 2 is inside MAX_PARALLAX_BANDS (16) and outside this config's count (2).
        let e = nudge(&mut f, field, 2, 3).expect_err("band 2 of a 2-band config does not exist");
        assert_eq!(e.reason.as_deref(), Some("bandOutsideConfig"));
        assert!(
            e.message.contains("2") && e.message.contains("16"),
            "the refusal must name BOTH numbers, or a reader cannot see that they differ: {}",
            e.message
        );
        assert!(
            f.writes().is_empty(),
            "nothing may be written: {:?}",
            f.writes()
        );
        // …and the band below it is accepted, so the bound is a bound and not a blanket refusal.
        let mut g = armed(2);
        nudge(&mut g, field, 1, 3).expect("band 1 of a 2-band config exists");
        assert_eq!(g.writes().len(), 1);
    }

    /// ⚑ **A VALUE OUTSIDE A FIELD'S COHERENT RANGE IS REFUSED, NOT CLIPPED.**
    ///
    /// The door accepts every byte and the engine reads whatever is there, so clipping would silently
    /// substitute a number the person did not ask for and the readback would then agree with the
    /// substitution. `factor_ops`'s bits 2-7 are unread: 4 is not a louder 3, it is 0 with a bit nobody
    /// looks at.
    #[test]
    fn a_value_outside_the_fields_coherent_range_is_refused_rather_than_clipped() {
        let mut f = armed(2);
        let field = FIELDS.iter().find(|f| f.key == "factor_ops").unwrap();
        let e = nudge(&mut f, field, 0, 4).expect_err("ops is 0..=3");
        assert_eq!(e.reason.as_deref(), Some("valueOutsideRange"));
        assert!(
            f.writes().is_empty(),
            "nothing may be written: {:?}",
            f.writes()
        );
        // The shift fields' 15 IS in range, because it is a documented sentinel rather than an overflow.
        let mut g = armed(2);
        let shift = FIELDS.iter().find(|f| f.key == "factor_a_s1").unwrap();
        nudge(&mut g, shift, 0, 15).expect("15 is the locked sentinel, not out of range");
        assert_eq!(g.writes(), vec![(SCRATCH.to_string(), 32, 15, 1)]);
    }

    /// ⚑ **THE CURSOR GUARD COVERS A SCRATCH WRITE TOO.**
    ///
    /// [`forbidden`] exists because writing `Debug_Lab_Index` moves a label and changes nothing that runs.
    /// A nudge is a write like any other and is passed through the same guard **on its resolved
    /// destination**, so a build whose scratch plus offset lands on the cursor is refused. The guard would
    /// otherwise cover only the channels, which is the half of the surface it was written for.
    #[test]
    fn the_lab_index_guard_fires_on_a_scratch_write_that_resolves_to_the_cursor() {
        let mut f = armed(2);
        // A listing whose scratch sits so that band 0's `factor_a_s1` (header 30 + 2) is the cursor.
        // ⚑ The END mark moves with it, so the SPAN is unchanged: a fixture that moved only the base
        // would be refused by the factoring check and would report a green on the wrong guard.
        let base = S4_LAB_INDEX - 32;
        let end = base + 542;
        f.listing
            .retain(|(n, _, _)| *n != SCRATCH && *n != SCRATCH_END && *n != SCRATCH_ARM);
        f.listing.push((SCRATCH, base & 0x00FF_FFFF, base));
        f.listing.push((SCRATCH_END, end & 0x00FF_FFFF, end));
        f.listing.push((SCRATCH_ARM, end & 0x00FF_FFFF, end));
        f.reads.retain(|(k, _)| k != "Parallax_Current_Config");
        f.reads
            .push(("Parallax_Current_Config".into(), format!("0x{base:08X}")));
        let field = FIELDS.iter().find(|f| f.key == "factor_a_s1").unwrap();
        let e = nudge(&mut f, field, 0, 3).expect_err("the cursor is never a nudge destination");
        assert_eq!(e.reason.as_deref(), Some("labIndexIsNotASelector"));
        assert!(
            f.writes().is_empty(),
            "nothing may be written: {:?}",
            f.writes()
        );
    }

    /// ⚑ **THE ARM IS A POKE PLUS ONE FRAME, AND EXACTLY ONE.**
    ///
    /// `NOTE` §6.2: an Aether client *"can write a byte and run a frame and cannot force a `jsr`"*, and one
    /// frame is enough rather than two because `Parallax_Update` polls the arm **at its head, ahead of its
    /// own config select** — so the install lands on that frame. A panel that ran two would be right by
    /// accident and would have stepped the game past what the person was looking at; one that ran none
    /// would read the ROM pointer back and report the hook as dead.
    #[test]
    fn arming_writes_the_request_byte_and_runs_exactly_one_frame() {
        let mut f = armed(2);
        let (_h, state) = arm(&mut f).expect("the s4-shaped listing is armable");
        assert_eq!(
            f.writes(),
            vec![(SCRATCH_ARM.to_string(), 0, 1, 1)],
            "one write, one byte, nonzero, into the request cell and nothing else"
        );
        assert_eq!(f.frames, 1, "exactly one frame");
        assert!(state.took(), "{}", state.line());
        // The order matters as much as the count: a frame run BEFORE the poke services nothing.
        let poke = f
            .calls
            .iter()
            .position(|(m, _)| m == "emulator/write_memory")
            .expect("the poke happened");
        let frame = f
            .calls
            .iter()
            .position(|(m, _)| m == "emulator/run_frames")
            .expect("the frame happened");
        assert!(
            poke < frame,
            "the request byte must be written BEFORE the frame: {:?}",
            f.calls
        );
    }

    /// ⚑ **THE SCRATCH READ STOPS AT THE INSTALLED BAND COUNT, NOT AT THE RESERVATION.**
    ///
    /// The install copies this config's own bands, so the bytes past them were never written. Reading the
    /// whole 542 and showing them as field values would be the panel presenting uninitialised RAM as a
    /// scene's numbers — a readout that lies, which is the one thing this surface may not do.
    #[test]
    fn the_scratch_read_stops_at_the_installed_band_count() {
        let mut f = armed(2);
        let h = hook(&mut f).expect("the listing");
        let s = read_scratch(&mut f, &h).expect("the scratch reads back");
        assert_eq!(s.band_count, 2);
        assert_eq!(
            s.raw.len(),
            30 + 32 * 2,
            "header plus TWO records, not header plus sixteen"
        );
        let lens: Vec<u64> = f
            .calls
            .iter()
            .filter(|(m, p)| m == "emulator/read_memory" && p["symbol"] == json!(SCRATCH))
            .map(|(_, p)| p["len"].as_u64().unwrap_or(0))
            .collect();
        assert!(
            lens.iter().all(|l| *l <= 94),
            "no read may ask for more than the install wrote: {lens:?}"
        );
    }

    /// ⚑ **THE NOTE'S ADDRESS IS A WITNESS AND THE LISTING HAS ALREADY MOVED PAST IT.**
    ///
    /// This is the measurement that decides the parcel's shape, so it is a row rather than a comment.
    /// The channel selectors used to REFUSE when a listing and the note disagreed about an address, and
    /// copying that discipline here would have been the obvious thing — and would refuse the whole feature
    /// on a healthy build: `Parallax_Scratch_Config` is at the RAM tail inside a size-varying
    /// `@shape_divergent` group and has already moved $20 since `NOTE` §6.1 recorded it. ⚑ **That
    /// refusal is gone** ([`Channel::witness`]) for the same finding, arrived at independently on the
    /// three selectors; this row is where it was measured first.
    ///
    /// So the gate is: the note's number and the listing's differ, **and the hook still works**.
    #[test]
    fn the_noted_scratch_address_is_a_witness_and_a_listing_that_has_moved_past_it_still_works() {
        assert_ne!(
            SCRATCH_NOTED_ADDR, S4_SCRATCH,
            "if these ever agree this row has stopped measuring anything and must be re-derived from the \
             listing rather than deleted"
        );
        let mut f = armed(2);
        let h = hook(&mut f).expect(
            "a listing whose scratch has moved since the note is a HEALTHY listing, not a drifted one",
        );
        assert_eq!(h.scratch_raw, S4_SCRATCH);
    }

    /// ⚑ **THE STALE FORECAST, CORRECTED WHERE A READER LOOKS FOR IT.**
    ///
    /// The switchboard design's §5.2 promised, by name, that the nudge control would be *"two numbers,
    /// `driver` and `rate shift`"* when the hook landed. **The hook that landed reaches a different
    /// struct.** Those two are BgAnim band-record fields and this is the parallax config's RAM copy.
    ///
    /// The refusal reason matters as much as the refusal: `driver` is **not** geometry — it is perfectly
    /// nudgeable in principle — so recording it under the geometry reason would be filing a correct
    /// conclusion under a wrong premise, and the next lane to ask would be told the wrong thing.
    #[test]
    fn the_refused_list_names_driver_and_rate_shift_as_the_wrong_channel_and_not_as_geometry() {
        for want in ["driver", "rate_shift"] {
            let row = NOT_OFFERED
                .iter()
                .find(|n| n.field == want)
                .unwrap_or_else(|| {
                    panic!("{want} must be listed, or its absence reads as an oversight")
                });
            assert_eq!(
                row.class, "wrong channel",
                "{want} is not geometry and not inert: it is a field of a struct this hook does not reach"
            );
            assert!(
                row.why.contains("ROM"),
                "the reason must say WHY it cannot be reached — the table it lives in is ROM: {}",
                row.why
            );
        }
        assert!(
            NOT_OFFERED
                .iter()
                .any(|n| n.field.contains("step_mask") && n.class == "geometry"),
            "and the two that ARE geometry must still be filed as geometry"
        );
        assert!(
            !FIELDS.iter().any(|f| f.key.contains("driver")
                || f.key.contains("rate_shift")
                || f.key.contains("step_mask")),
            "none of them may be offered"
        );
    }

    /// ⚑ **THE GATE M18 FOUND MISSING, AND THE MUTATION THAT READ GREEN IS THE REASON IT EXISTS.**
    ///
    /// The campaign's M18 changed [`resolved_offsets`] from *drop a field whose equate is unpublished* to
    /// *default it to zero*, and **the suite stayed green**: the row on `nudge` covers the WRITE path,
    /// which resolves the equate itself and refuses, and nothing covered the **DRAW** path, which reads its
    /// offsets out of [`Nudging::offsets`].
    ///
    /// That gap is not cosmetic. Offset 0 is `pcfg_band_count`, so a defaulted zero makes the control for
    /// an unpublished field **display the band count as its value** — a readout that lies, on the surface
    /// whose entire thesis is that it does not, with a correct refusal on the write hiding it from the one
    /// row that would have noticed.
    ///
    /// So both directions are asserted here: absent means **absent**, and present means the byte the
    /// listing's offset names.
    #[test]
    fn a_field_whose_equate_is_unpublished_has_no_offset_rather_than_a_defaulted_zero() {
        let field = FIELDS
            .iter()
            .find(|f| f.key == "layer_mask")
            .expect("the field exists");

        // Present: the offset is the listing's (2) and the value is the byte there.
        let mut f = armed(2);
        let h = hook(&mut f).expect("the listing");
        let n = Nudging {
            hook: h,
            installed: took(&mut f, &h).expect("the state"),
            scratch: Some(read_scratch(&mut f, &h).expect("the bytes")),
            offsets: resolved_offsets(&mut f, &h),
        };
        assert_eq!(n.equate_of(field), Some(2));
        // `scratch_bytes` puts `i ^ 0x5A` at byte i, so the u16 at offset 2 is $5859 — a value no other
        // offset in the buffer holds, which is why the fixture uses distinct bytes rather than a pattern.
        assert_eq!(
            n.value_of(field, 0),
            Some(0x5859),
            "a present equate must read the word the LISTING's offset names"
        );

        // Absent: no entry at all, so the control has nothing to draw and says so.
        let mut g = armed(2).without_equate("parallax_config_pcfg_layer_mask");
        let h = hook(&mut g).expect("the listing still factors");
        let n = Nudging {
            hook: h,
            installed: took(&mut g, &h).expect("the state"),
            scratch: Some(read_scratch(&mut g, &h).expect("the bytes")),
            offsets: resolved_offsets(&mut g, &h),
        };
        assert_eq!(
            n.equate_of(field),
            None,
            "an unpublished equate must leave NO entry. A defaulted 0 is `pcfg_band_count`'s offset, so \
             the control would show the band count as this field's value"
        );
        assert_eq!(
            n.value_of(field, 0),
            None,
            "and therefore no value, so the control draws unavailable rather than confidently wrong"
        );
        // The other fields are unaffected, so this is a per-field absence and not a collapsed surface.
        let other = FIELDS.iter().find(|f| f.key == "factor_a_s1").unwrap();
        assert_eq!(n.equate_of(other), Some(2));
    }

    /// ⚑ **THE MOVED ADDRESS IS SAID ON SCREEN, not only in a comment.**
    ///
    /// A reader comparing this panel against `NOTE` §6.1 finds the two addresses disagree, so the shape
    /// line names the difference and says why it is benign here. Left out, the most likely reading of a
    /// discrepancy is the one this module used to refuse everywhere else for, which is an hour.
    #[test]
    fn the_shape_line_names_the_notes_address_when_the_listing_has_moved_past_it() {
        let mut f = armed(2);
        let h = hook(&mut f).expect("the listing");
        let n = Nudging {
            hook: h,
            installed: took(&mut f, &h).expect("the state"),
            scratch: None,
            offsets: Vec::new(),
        };
        let line = n.shape_line();
        // ⛑ **EXACT PHRASES, NOT LOOSE DIGITS**, and the first version of this row got it wrong. It
        // read `for want in ["542", "30", "16", "32"]`, and **M20 (printing the ceiling where the stride
        // belongs, so the line says "16 records of 16 bytes") PASSED it** — because every digit that
        // matters also occurs as a LITERAL in the same sentence ("it is 32 on s4.debug and 10 on
        // demo.debug"). The question to ask of an assertion over rendered text carrying numbers is what
        // the SPACE of values is, not which value was seen: over that space a bare-digit substring test
        // here cannot fail at all.
        for want in [
            "542 bytes",
            "30-byte header and 16 records of 32 bytes",
            "(542 - 30) / 16",
        ] {
            assert!(
                line.contains(want),
                "the derivation must be checkable, and {want:?} is the phrase that carries it: {line}"
            );
        }
        assert!(
            line.contains(&format!("{SCRATCH_NOTED_ADDR:#010X}")),
            "the note's address must appear, because a reader will find the disagreement anyway: {line}"
        );
        assert!(
            line.contains("nothing refuses on it"),
            "and it must say the disagreement is deliberately not a refusal, which is what a reader \
             coming from this module's older shape will otherwise assume it should be: {line}"
        );
    }

    /// ⚑ **NOTHING OFFERED IS A FIELD THE NOTE CALLS INERT OR COUPLED.**
    ///
    /// `NOTE` §6.5's three-way division is the whole reason [`FIELDS`] is short: an **inert** field is a
    /// slider that does nothing, which §0 exists to prevent, and a **coupled** field produces a picture
    /// whose parts disagree, which is subtler and worse. The check runs over the equate names rather than
    /// the labels, because the equate name is what the write actually resolves.
    #[test]
    fn no_offered_field_is_one_the_note_calls_inert_or_coupled() {
        // Straight from `NOTE` 6.3/6.5, spelled as the engine spells them.
        for barred in [
            "pcfg_v_factor_fg",
            "pcfg_v_factor_bg",
            "pcfg_v_center_y",
            "pcfg_v_offset",
            "pcfg_transition",
            "pcfg_band_count",
            "band_top_plane",
            "brm_hshift",
            "bc_step",
            "bc_rem",
            "bc_span",
            "bc_pad",
        ] {
            assert!(
                !FIELDS.iter().any(|f| f.equate == barred),
                "{barred} is offered, and NOTE 6.5 puts it in the inert or coupled class"
            );
            assert!(
                NOT_OFFERED.iter().any(|n| n.field.contains(barred)),
                "{barred} is neither offered nor explained, so its absence reads as an oversight"
            );
        }
        // And every offered field must actually resolve in a real listing's equate namespace, or the
        // control draws permanently unavailable and nobody finds out why.
        let published: Vec<&str> = scratch_equates().into_iter().map(|(n, _)| n).collect();
        for f in FIELDS {
            assert!(
                published.contains(&f.equate_name().as_str()),
                "`{}` is offered but s4.debug.lst publishes no `{}` — the offsets are the listing's, so a \
                 field whose equate is not published can never be written",
                f.label,
                f.equate_name()
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
