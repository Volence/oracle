//! **Placing a ring** — the one thing on this window's glass that is not an object, and the two rules
//! that make placing it destructive if they are got wrong.
//!
//! # ⚑ A ring is not an object, and that is the first thing to know
//!
//! Everything [`crate::spawn`] places goes through `emulator/object_spawn`, which writes aeon's
//! `Obj_Req_*` mailbox and gets back a slot in the SST pool. **A ring never touches any of that.** It has
//! no `ObjDef_`, it takes no pool slot, and it is not in the archetype list a click cycles through: the
//! six archetypes this build publishes are `Spring`, `PathSwap`, `Static`, `Solid`, `Enemy` and `Parent`,
//! and there is no `ObjDef_Ring` to select. Rings live in one flat array, `Ring_Buffer`, walked by
//! `DrawRings` straight into the sprite table and by `RingCollision` for the pickup. So placing one is a
//! **buffer write**, not a mailbox request, and it needs its own choreography rather than a row in
//! somebody else's list.
//!
//! # The two rules the symbol channel could not carry
//!
//! aeon publishes the ring layout as equates, and this module reads every bound rather than typing it, on
//! [`crate::spawn::ARCHETYPE_PREFIX`]'s standing rule. **But the listing carries `EQU <name> = $<value>`
//! and nothing else** — no comments — so the two facts below had no route from that repo to this one
//! except prose, and they arrived as `docs/2026-09-06-ring-placement-rules.md`.
//!
//! ## Rule 1: a placed ring is swept once the camera leaves it, by design
//!
//! `EntityWindow_DespawnRings` removes any buffer record that leaves the camera window, and
//! `EntityWindow_TrySpawnRing` only ever re-adds rings **from the section's ROM list**. A record with no
//! ROM entry behind it therefore does not come back. [`TEMPORARY`] is that sentence, and it is drawn on
//! the glass as **a condition of this feature** rather than as a polish item: a ring that vanishes the
//! first time the camera moves, with no explanation, reads as a broken tool rather than as the design.
//!
//! ## Rule 2: an index below the section's real ring count breaks a DIFFERENT ring
//!
//! A record carries `(section_id, list_index)`, and collecting it calls `Collected_MarkRing` with that
//! pair, which sets bit `list_index` in that section's collected mask. `EntityWindow_TrySpawnRing` reads
//! the same bit before offering a real ring. **So a `list_index` below the section's real ring count
//! marks a real ring collected, and that ring stops appearing.** ⚑ The damage lands on a different object
//! at a different time and nothing about the symptom points back at this window.
//!
//! **The count is not stored anywhere**, so [`census`] walks the section's ROM list to measure it.
//!
//! There is a second, quieter reason for the upper bound that the handoff does not name and that this
//! module derives instead: `EntityLoaded_Set`/`_Clear` index a per-section bitmask whose ring half is
//! exactly `MAX_LIST_ENTRIES` bits wide (aeon's own `ensure(ENTITY_LOADED_OBJ_OFFSET*8 ==
//! MAX_LIST_ENTRIES)`), so an index at or above it spills out of the ring half and into the object half.
//! One bound, two ways to be wrong past it.
//!
//! # Nothing here types a number
//!
//! ⚑ **`MAX_RING_BUFFER` is `$80` on sonic4 and `$10` on demo.** Every bound in [`Layout`] is read from
//! the listing through `emulator/lookup_equate`, and a bound the listing does not publish is a **stated
//! refusal** rather than a default: a picker correct on one game and silently wrong on the other is the
//! kind of bug nobody finds.
//!
//! ⚑ **Read through `emulator/lookup_equate`, NEVER `emulator/lookup_symbol`.** The two read disjoint
//! sections of the listing, and a symbol query against an equate refuses in a way that reads *exactly*
//! like the name not existing. That distinction has already cost two lanes real time.
//!
//! # No new bus method, and the gap that leaves
//!
//! Every step here is a method the bus already serves: `lookup_equate` for the bounds, `lookup_symbol`
//! for the three RAM labels, `read_memory` for the scan state and the ROM list, `write_memory` for the
//! record. So this needs nothing new on the wire — **and that means a socket or MCP client cannot do it
//! without reimplementing this whole walk**, which is a capability that exists on one surface only.
//! Booked as a follow-up naming a possible `emulator/ring_place`, deliberately, so the gap is a decision
//! rather than an omission.

#[cfg(feature = "aether")]
use crate::spawn::Caller;
use crate::spawn::Refusal;

// ---------------------------------------------------------------------------------------------------
// The names. Every one of them is a name; not one of them is a value.
// ---------------------------------------------------------------------------------------------------

/// `Ring_Buffer` — the flat array of live ring records, in work RAM.
pub const RING_BUFFER_SYMBOL: &str = "Ring_Buffer";
/// `Ring_Count` — one byte, how many records are live.
pub const RING_COUNT_SYMBOL: &str = "Ring_Count";
/// `Ring_HighWater` — one byte, the largest [`RING_COUNT_SYMBOL`] has ever reached.
///
/// Written here for one reason: `RingBuffer_Add` maintains it, this module stands in for
/// `RingBuffer_Add`, and **the player's own Objects panel reads it**. A placement that bumped the count
/// and left the high water mark behind would make one of our own readouts quietly wrong about a machine
/// we had just written to.
pub const RING_HIGH_WATER_SYMBOL: &str = "Ring_HighWater";
/// `Entity_Scan_State` — `MAX_TRACKED_SECTIONS` records of `EntityScanState_len` bytes, one per section
/// the camera window is currently tracking.
pub const SCAN_STATE_SYMBOL: &str = "Entity_Scan_State";

/// Every equate [`Layout`] is built out of, in the listing's own spelling.
///
/// A list rather than fifteen literals at their use sites, so [`Layout::read`] can report **all** the
/// missing ones in one sentence: an absence answered one name at a time is a person running the same
/// gesture fifteen times to discover fifteen facts.
pub const EQUATES: &[&str] = &[
    "MAX_RING_BUFFER",
    "MAX_LIST_ENTRIES",
    "MAX_TRACKED_SECTIONS",
    "RING_BUFFER_ENTRY_SIZE",
    "RING_ENTRY_X_OFFSET",
    "RING_ENTRY_Y_OFFSET",
    "RING_ENTRY_SECTION_ID_OFFSET",
    "RING_ENTRY_LIST_INDEX_OFFSET",
    "RING_LIST_ENTRY_SIZE",
    "RING_LIST_TERMINATOR",
    "EntityScanState_len",
    "EntityScanState_ess_rom_ring_ptr",
    "EntityScanState_ess_section_id",
    "EntityScanState_ess_entry_idx",
    "EntityScanState_ess_origin_x",
    "EntityScanState_ess_origin_y",
    "SECTION_SIZE",
    "SEC_VOID",
];

/// ⚑ **The sentence a placed ring owes the person who placed it**, drawn on the glass for as long as
/// ring placement is armed.
///
/// A standing statement rather than a toast, on the spawn badge's own rule: a toast expires and the
/// design does not. It is ruled with the feature rather than added to it, because the alternative is a
/// ring that disappears the first time the camera moves and a person who reasonably concludes the window
/// is broken.
///
/// It says *the camera moving away* rather than *leaving the screen* because that is what the engine
/// actually does: `EntityWindow_DespawnRings` keeps a record while its X is inside a window wider than
/// the screen by `ENTITY_DESPAWN_BUFFER` on each side, or while its section is still one of the tracked
/// ones and its Y is inside the despawn band. Saying "the moment it leaves the screen" would be a
/// crisper sentence and a false one.
pub const TEMPORARY: &str =
    "A RING YOU PLACE IS TEMPORARY. The engine sweeps it out of the ring buffer once the camera moves \
     away from it, and nothing brings it back, because rings only ever respawn from the level's own \
     list in the cartridge and this one is not in that list. That is the engine's design and not a \
     fault in this window. Nothing here writes into the level.";

/// The largest `list_index` a record can carry, because the field is one byte.
///
/// Not a bound on where a ring may go: [`Layout::max_list_entries`] is that, and it is read. This is the
/// **field width**, which is what makes an index the listing would otherwise permit unwritable, and it is
/// checked so that a build raising `MAX_LIST_ENTRIES` past 255 is refused out loud instead of having its
/// index silently truncated into a different ring's bit.
pub const MAX_LIST_INDEX_IN_A_BYTE: u64 = 255;

// ---------------------------------------------------------------------------------------------------
// Layout — every bound, read
// ---------------------------------------------------------------------------------------------------

/// **The ring format, as the loaded listing publishes it.** Never a constant, never a default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// `MAX_RING_BUFFER`: how many records fit. `$80` on sonic4, `$10` on demo.
    pub max_ring_buffer: u64,
    /// `MAX_LIST_ENTRIES`: the exclusive ceiling on a `list_index`.
    pub max_list_entries: u64,
    /// `MAX_TRACKED_SECTIONS`: how many scan-state records `Entity_Scan_State` holds.
    pub max_tracked_sections: u64,
    /// `RING_BUFFER_ENTRY_SIZE`: the stride between records, and the length of the write.
    pub entry_size: u64,
    /// `RING_ENTRY_X_OFFSET` / `_Y_` / `_SECTION_ID_` / `_LIST_INDEX_`, within one record.
    pub x_off: u64,
    pub y_off: u64,
    pub section_id_off: u64,
    pub list_index_off: u64,
    /// `RING_LIST_ENTRY_SIZE`: the stride of the section's ROM list.
    pub list_entry_size: u64,
    /// `RING_LIST_TERMINATOR`: the long that ends that list.
    pub list_terminator: u64,
    /// `EntityScanState_len`: the stride between scan-state records.
    pub scan_len: u64,
    /// The four scan-state fields this module reads, at their published offsets.
    pub scan_rom_ring_ptr_off: u64,
    pub scan_section_id_off: u64,
    pub scan_entry_idx_off: u64,
    pub scan_origin_x_off: u64,
    pub scan_origin_y_off: u64,
    /// `SECTION_SIZE`: how many world pixels a section covers on each axis.
    pub section_size: u64,
    /// `SEC_VOID`: the id a scan-state slot carries when it is tracking nothing.
    pub sec_void: u64,
}

impl Layout {
    /// Build one from a name-to-value map, refusing when a name is absent or the shape is impossible.
    ///
    /// **Two kinds of refusal and they are different findings.** A missing name is a listing that cannot
    /// answer; a record whose published offsets do not fit inside its published size is a listing that
    /// answers inconsistently, and composing a record from it would put a byte somewhere nothing asked
    /// for. Neither is defaulted.
    pub fn from_values(v: &std::collections::BTreeMap<String, u64>) -> Result<Self, Refusal> {
        let missing: Vec<&str> = EQUATES
            .iter()
            .copied()
            .filter(|n| !v.contains_key(*n))
            .collect();
        if !missing.is_empty() {
            return Err(Refusal::window(
                "ringBoundsUnknown",
                format!(
                    "the window cannot place a ring in this build: its listing does not publish {}. \
                     Every bound a ring record needs is read from the listing rather than written down \
                     here, because they differ between games (the ring buffer holds 128 records in one \
                     of this engine's games and 16 in the other), so a missing name is a measurement \
                     this window does not have rather than one it can assume",
                    missing.join(", ")
                ),
                Some(
                    "load a listing from a build that publishes the ring layout, then click again"
                        .to_string(),
                ),
            ));
        }
        let g = |n: &str| *v.get(n).expect("checked present just above");
        let l = Layout {
            max_ring_buffer: g("MAX_RING_BUFFER"),
            max_list_entries: g("MAX_LIST_ENTRIES"),
            max_tracked_sections: g("MAX_TRACKED_SECTIONS"),
            entry_size: g("RING_BUFFER_ENTRY_SIZE"),
            x_off: g("RING_ENTRY_X_OFFSET"),
            y_off: g("RING_ENTRY_Y_OFFSET"),
            section_id_off: g("RING_ENTRY_SECTION_ID_OFFSET"),
            list_index_off: g("RING_ENTRY_LIST_INDEX_OFFSET"),
            list_entry_size: g("RING_LIST_ENTRY_SIZE"),
            list_terminator: g("RING_LIST_TERMINATOR"),
            scan_len: g("EntityScanState_len"),
            scan_rom_ring_ptr_off: g("EntityScanState_ess_rom_ring_ptr"),
            scan_section_id_off: g("EntityScanState_ess_section_id"),
            scan_entry_idx_off: g("EntityScanState_ess_entry_idx"),
            scan_origin_x_off: g("EntityScanState_ess_origin_x"),
            scan_origin_y_off: g("EntityScanState_ess_origin_y"),
            section_size: g("SECTION_SIZE"),
            sec_void: g("SEC_VOID"),
        };
        l.check()?;
        Ok(l)
    }

    /// **The published numbers have to describe a record this window can actually compose.**
    ///
    /// Derived, every clause of it: two-byte fields must fit inside the published entry size at their
    /// published offsets, and so must the two one-byte fields. Nothing here compares against a literal
    /// six. A build that reorders the record keeps working; a build whose numbers contradict each other
    /// is refused with the numbers quoted, rather than having a byte written past the end of a record and
    /// into the next ring.
    fn check(&self) -> Result<(), Refusal> {
        let mut bad: Vec<String> = Vec::new();
        for (name, off, width) in [
            ("RING_ENTRY_X_OFFSET", self.x_off, 2),
            ("RING_ENTRY_Y_OFFSET", self.y_off, 2),
            ("RING_ENTRY_SECTION_ID_OFFSET", self.section_id_off, 1),
            ("RING_ENTRY_LIST_INDEX_OFFSET", self.list_index_off, 1),
        ] {
            if off + width > self.entry_size {
                bad.push(format!(
                    "{name} is {off} and the field is {width} byte(s), which runs past the end of a \
                     {}-byte record",
                    self.entry_size
                ));
            }
        }
        if self.list_entry_size == 0 || self.entry_size == 0 || self.scan_len == 0 {
            bad.push(
                "a stride of zero would make a walk over the list never advance, so nothing is walked"
                    .to_string(),
            );
        }
        if self.section_size == 0 {
            bad.push(
                "SECTION_SIZE is zero, so no section covers any pixel and no click could be inside one"
                    .to_string(),
            );
        }
        if self.max_list_entries > MAX_LIST_INDEX_IN_A_BYTE + 1 {
            bad.push(format!(
                "MAX_LIST_ENTRIES is {}, and a record carries its list index in one byte, so the top \
                 of that range cannot be written without cutting the index down into a different \
                 ring's collected bit",
                self.max_list_entries
            ));
        }
        if bad.is_empty() {
            return Ok(());
        }
        Err(Refusal::window(
            "ringRecordLayoutUnknown",
            format!(
                "this listing's ring numbers do not describe a record this window can compose, so \
                 nothing was written: {}",
                bad.join("; ")
            ),
            None,
        ))
    }

    /// The address of buffer record `i`, from the base [`RING_BUFFER_SYMBOL`] resolved to.
    pub fn record_addr(&self, base: u32, i: u64) -> u64 {
        u64::from(base) + i * self.entry_size
    }

    /// **One record's bytes**, each field placed at its own published offset.
    ///
    /// Composed rather than concatenated, which is the whole point: a build that swapped the section id
    /// and the list index would move both bytes here without a line changing, and a build whose offsets
    /// no longer fit was already refused by [`Layout::check`]. The two-byte fields are big-endian, as the
    /// 68000 stores.
    pub fn record(&self, x: u16, y: u16, section_id: u8, list_index: u8) -> Vec<u8> {
        let mut r = vec![0u8; self.entry_size as usize];
        r[self.x_off as usize..self.x_off as usize + 2].copy_from_slice(&x.to_be_bytes());
        r[self.y_off as usize..self.y_off as usize + 2].copy_from_slice(&y.to_be_bytes());
        r[self.section_id_off as usize] = section_id;
        r[self.list_index_off as usize] = list_index;
        r
    }
}

// ---------------------------------------------------------------------------------------------------
// The tracked sections, and the census of one of them
// ---------------------------------------------------------------------------------------------------

/// **One slot of `Entity_Scan_State`** — a section the camera window is tracking right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Section {
    /// `ess_entry_idx`, the window entry this slot is.
    pub entry: u8,
    /// `ess_section_id`, the flat section id a record would carry.
    pub id: u8,
    /// `ess_origin_x` / `ess_origin_y`, the section's top-left in world pixels.
    pub origin: (u16, u16),
    /// `ess_rom_ring_ptr`, where the section's ROM ring list starts. **Zero means the slot cannot be
    /// measured** — see [`Section::has_list`].
    pub rom_ring_ptr: u32,
}

impl Section {
    /// Whether the world pixel `(x, y)` is inside this section's `section_size` square.
    ///
    /// Half-open on the high edge, matching how the engine derives a section from a coordinate: the
    /// pixel at `origin + section_size` is the first one belonging to the next section along.
    pub fn contains(&self, world: (u32, u32), section_size: u64) -> bool {
        let (x, y) = world;
        let (ox, oy) = (u64::from(self.origin.0), u64::from(self.origin.1));
        let (x, y) = (u64::from(x), u64::from(y));
        x >= ox && x < ox + section_size && y >= oy && y < oy + section_size
    }

    /// ⚑ **Whether this slot's ring list can be measured at all.**
    ///
    /// A null `ess_rom_ring_ptr` has **two** causes and this window cannot tell them apart: a section
    /// that genuinely publishes no rings (`EntityWindow_ScanRingsRight`'s own *"null list: no rings ever
    /// enter"*), and a scan-state slot still holding the zeroes `EntityWindow_Init` cleared it to, which
    /// also reads as section id 0 at origin (0, 0) with a null list. The second is the dangerous one:
    /// treating it as a count of zero would place a ring at index 0 of section 0 and, the moment the
    /// level really loaded section 0, that would be the aliasing this module exists to prevent.
    ///
    /// So both are refused. Losing the genuinely-empty section costs one placement; guessing costs a
    /// ring the person will never find again.
    pub fn has_list(&self) -> bool {
        self.rom_ring_ptr != 0
    }
}

/// **What a section's ROM ring list says**, plus what is already sitting in the live buffer for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Census {
    /// The section this is about.
    pub section: Section,
    /// **The section's real ring count**, walked rather than read: it is not stored anywhere.
    pub real_rings: u64,
    /// The `list_index` of every live buffer record already carrying this section's id, so a second
    /// placement does not land on the first one's bit.
    pub taken: Vec<u64>,
    /// How many records the whole buffer is holding, across every section.
    pub buffer_used: u64,
}

impl Census {
    /// **The lowest `list_index` it is safe to place at**, or `None` when there is no such index.
    ///
    /// At or above [`Census::real_rings`], below `max_list_entries`, and not one this window has already
    /// used in this section. `None` is the second of the two "cannot place" states and it is a genuinely
    /// different fact from a full buffer: the buffer may have room and this section still have no index
    /// left to give.
    pub fn free_index(&self, layout: &Layout) -> Option<u64> {
        (self.real_rings..layout.max_list_entries).find(|i| !self.taken.contains(i))
    }

    /// The refusal for a section with no free index. Says the two numbers it was decided from.
    ///
    /// ⚑ **The clause about the buffer is DERIVED, not asserted.** The obvious wording is *"the ring
    /// buffer is not the problem here"*, and it is a lie exactly when both are full at once. The point
    /// of splitting these two states is that a person reads one and knows what to fix; a sentence that
    /// tells them the buffer is fine while the buffer is full sends them to fix a thing that is not
    /// broken and leaves the thing that is. So the clause is computed from the same numbers the state
    /// was, and it cannot say the buffer is fine unless it is.
    pub fn no_index(&self, layout: &Layout) -> Refusal {
        let buffer = if self.buffer_used >= layout.max_ring_buffer {
            format!(
                "The ring buffer is full as well, at {} of {} records, so both limits are in the way \
                 at once.",
                self.buffer_used, layout.max_ring_buffer
            )
        } else {
            format!(
                "The ring buffer is not the problem here: it is holding {} of {} records.",
                self.buffer_used, layout.max_ring_buffer
            )
        };
        Refusal::window(
            "ringSectionFull",
            format!(
                "there is no index left in section {} for a ring to use, so nothing was placed. Its \
                 own level data already uses indexes 0 to {}, this window has since used {}, and the \
                 engine's ceiling for the section is {}. {buffer} An index below {} would be one the \
                 level's own rings answer to, and collecting a ring placed there would mark one of \
                 those real rings as collected and stop it appearing.",
                self.section.id,
                self.real_rings.saturating_sub(1),
                if self.taken.is_empty() {
                    "none".to_string()
                } else {
                    self.taken
                        .iter()
                        .map(u64::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                },
                layout.max_list_entries,
                self.real_rings,
            ),
            Some(
                "move the camera to a different section of the level, or restart the act to clear \
                 the rings this window has placed"
                    .to_string(),
            ),
        )
    }
}

/// The refusal for a full ring buffer. The **other** "cannot place", and it says so out loud so the two
/// are never read as one greyed-out control.
pub fn buffer_full(used: u64, layout: &Layout) -> Refusal {
    Refusal::window(
        "ringBufferFull",
        format!(
            "the ring buffer is full, so nothing was placed: it is holding {used} of the {} records \
             this build has room for. This is not the same as the section running out of indexes; the \
             section may have plenty. Every ring on screen is in this buffer, so collecting some or \
             moving the camera away from a crowd of them makes room.",
            layout.max_ring_buffer
        ),
        Some("collect some rings or move away from them, then click again".to_string()),
    )
}

/// The refusal for a click in a section the camera window is not tracking.
///
/// ⚑ **A third "cannot place", and the handoff names only two.** It is unavoidable rather than a design
/// choice: the section's ROM ring list is reachable only through a scan-state slot, so a section with no
/// slot has no measurable ring count, so no index can be shown to be safe. Refusing is the only honest
/// answer, and it is a friendly one, because the way out is to move the camera.
pub fn untracked(world: (u32, u32), sections: &[Section]) -> Refusal {
    let held = if sections.is_empty() {
        "it is tracking none at the moment, which is what it looks like before an act has started"
            .to_string()
    } else {
        format!(
            "it is tracking {}",
            sections
                .iter()
                .map(|s| format!("section {} from ({}, {})", s.id, s.origin.0, s.origin.1))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    Refusal::window(
        "ringSectionUntracked",
        format!(
            "world ({}, {}) is not in any section the engine is currently tracking, so nothing was \
             placed. A ring has to name the section it belongs to, and the only way this window can \
             tell how many rings that section really has is to read the list the engine is already \
             holding for it. For a section the engine is not holding there is no such list, and \
             guessing the count would risk stopping one of the level's own rings from appearing. \
             Right now {held}.",
            world.0, world.1
        ),
        Some("move the camera nearer the spot you want, then click again".to_string()),
    )
}

// ---------------------------------------------------------------------------------------------------
// A placed ring
// ---------------------------------------------------------------------------------------------------

/// **A ring that is now in the buffer**, and everything a reader needs to not be surprised by it later.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    /// The world pixel it went to, which for a ring **is** where it was asked for: nothing advances a
    /// frame here, so unlike an object spawn there is no re-read and no chance it has already moved.
    pub world: (u32, u32),
    /// The section id the record carries.
    pub section_id: u8,
    /// The list index the record carries, chosen by [`Census::free_index`].
    pub list_index: u64,
    /// The section's real ring count, which is the floor that index had to clear.
    pub real_rings: u64,
    /// The buffer slot the record was written into, and the address it went to.
    pub slot: u64,
    pub addr: u64,
    /// `Ring_Count` after the write, and the ceiling it is counting towards.
    pub buffer_used: u64,
    pub buffer_max: u64,
}

impl Placed {
    /// The full line for the terminal.
    ///
    /// ⚑ **It carries [`TEMPORARY`] verbatim.** The standing statement on the panel is the primary
    /// channel and this is the second one, because the person who reads a placement line and then scrolls
    /// is the exact person the rule is for. One wording, in one place, so the two cannot drift into two
    /// accounts of one design.
    pub fn terminal(&self) -> String {
        format!(
            "placed a ring at world ({}, {}): it is buffer slot {} at {:#010X}, and it belongs to \
             section {} as ring number {}. That number is at or above the {} ring(s) the level's own \
             data puts in this section, which is what keeps collecting it from marking one of those \
             real rings as collected. The buffer is now holding {} of {} records. {TEMPORARY}",
            self.world.0,
            self.world.1,
            self.slot,
            self.addr,
            self.section_id,
            self.list_index,
            self.real_rings,
            self.buffer_used,
            self.buffer_max,
        )
    }

    /// The short form for the toast.
    pub fn toast(&self) -> String {
        format!(
            "RING PLACED AT ({}, {}) SLOT {} OF {}: TEMPORARY, IT GOES WHEN THE CAMERA DOES",
            self.world.0, self.world.1, self.buffer_used, self.buffer_max
        )
    }
}

// ---------------------------------------------------------------------------------------------------
// The choreography
// ---------------------------------------------------------------------------------------------------

/// **Every bound this build publishes**, read through the equate door one name at a time.
///
/// ⚑ **`emulator/lookup_equate`, never `emulator/lookup_symbol`.** They read disjoint sections of the
/// listing and a symbol query against an equate refuses in a way that reads exactly like the name not
/// existing, which is how a lane concludes a build has no ring layout when it has published one all
/// along.
///
/// A refused name is **collected, not propagated**: the point of asking for all of them is to be able to
/// name all the missing ones at once, and an early return on the first refusal would report one name and
/// hide seventeen.
#[cfg(feature = "aether")]
pub fn layout(c: &mut impl Caller) -> Result<Layout, Refusal> {
    let mut found = std::collections::BTreeMap::new();
    for name in EQUATES {
        if let Ok(v) = c.call("emulator/lookup_equate", serde_json::json!({"name": name})) {
            if let Some(n) = v["value"].as_u64() {
                found.insert((*name).to_string(), n);
            }
        }
    }
    Layout::from_values(&found)
}

/// **The sections the camera window is tracking**, read out of `Entity_Scan_State`.
///
/// One read of the whole array rather than one per slot, so every slot describes the same instant. A slot
/// carrying `SEC_VOID` is tracking nothing and is dropped here rather than offered and refused later.
#[cfg(feature = "aether")]
pub fn sections(c: &mut impl Caller, l: &Layout) -> Result<Vec<Section>, Refusal> {
    let base = c.address_of(SCAN_STATE_SYMBOL).ok_or_else(|| {
        missing_symbol(SCAN_STATE_SYMBOL, "which sections the camera is tracking")
    })?;
    let span = l.scan_len * l.max_tracked_sections;
    let bytes = read(c, u64::from(base), span)?;
    let mut out = Vec::new();
    for i in 0..l.max_tracked_sections {
        let at = (i * l.scan_len) as usize;
        let b = |off: u64| bytes[at + off as usize];
        let w =
            |off: u64| u16::from_be_bytes([bytes[at + off as usize], bytes[at + off as usize + 1]]);
        let id = b(l.scan_section_id_off);
        if u64::from(id) == l.sec_void {
            continue;
        }
        let ptr = u32::from_be_bytes([
            bytes[at + l.scan_rom_ring_ptr_off as usize],
            bytes[at + l.scan_rom_ring_ptr_off as usize + 1],
            bytes[at + l.scan_rom_ring_ptr_off as usize + 2],
            bytes[at + l.scan_rom_ring_ptr_off as usize + 3],
        ]);
        out.push(Section {
            entry: b(l.scan_entry_idx_off),
            id,
            origin: (w(l.scan_origin_x_off), w(l.scan_origin_y_off)),
            rom_ring_ptr: ptr,
        });
    }
    Ok(out)
}

/// **Walk a section's ROM ring list and count it**, then read the live buffer for what is already taken.
///
/// The walk is `RING_LIST_ENTRY_SIZE` at a time from `ess_rom_ring_ptr` until a long equal to
/// `RING_LIST_TERMINATOR`, which is aeon's `EntityWindow_ScanRingsRight` and `PopulateSectionRings` doing
/// the identical thing.
///
/// ⚑ **The walk is bounded by `MAX_LIST_ENTRIES` and an unterminated list is UNMEASURABLE, not long.** A
/// pointer into the wrong part of the cartridge produces a plausible sequence of numbers that never ends;
/// stopping at the ceiling and calling that the count would hand back the ceiling as a measurement. It
/// refuses instead.
#[cfg(feature = "aether")]
pub fn census(c: &mut impl Caller, l: &Layout, s: Section) -> Result<Census, Refusal> {
    if !s.has_list() {
        return Err(Refusal::window(
            "ringListUnmeasurable",
            format!(
                "section {} does not point at a ring list, so this window cannot tell how many rings \
                 it really has and will not guess. A null pointer here means one of two things and \
                 nothing distinguishes them from outside: a section the level genuinely gives no \
                 rings, or a tracking slot the engine has cleared and not filled in yet. Reading it \
                 as a count of zero would risk placing a ring on top of a real ring's number and \
                 stopping that ring from ever appearing.",
                s.id
            ),
            Some("move the camera into a part of the level that has rings, then click again".to_string()),
        ));
    }
    // One entry past the ceiling, so a list that is exactly full is still seen to terminate.
    let span = (l.max_list_entries + 1) * l.list_entry_size;
    let bytes = read(c, u64::from(s.rom_ring_ptr), span)?;
    let mut real_rings = None;
    for i in 0..=l.max_list_entries {
        let at = (i * l.list_entry_size) as usize;
        // The terminator is compared over one whole list entry, which is what the engine's `move.l (a0)`
        // does. Reading only the X word would end the walk at the first ring whose X happens to be zero.
        let mut word = 0u64;
        for k in 0..l.list_entry_size as usize {
            word = (word << 8) | u64::from(bytes[at + k]);
        }
        if word == l.list_terminator {
            real_rings = Some(i);
            break;
        }
    }
    let Some(real_rings) = real_rings else {
        return Err(Refusal::window(
            "ringListUnmeasurable",
            format!(
                "section {}'s ring list does not end within the {} entries this engine allows, so \
                 this window cannot tell how many rings the section really has. A list that never \
                 ends usually means the pointer is not pointing at a list at all. Nothing was placed, \
                 because every safe position for a new ring is measured from that count.",
                s.id, l.max_list_entries
            ),
            None,
        ));
    };

    let (buffer_used, taken) = live(c, l, s.id)?;
    Ok(Census {
        section: s,
        real_rings,
        taken,
        buffer_used,
    })
}

/// `Ring_Count`, and the `list_index` of every live record already carrying `section_id`.
#[cfg(feature = "aether")]
fn live(c: &mut impl Caller, l: &Layout, section_id: u8) -> Result<(u64, Vec<u64>), Refusal> {
    let count_addr = c
        .address_of(RING_COUNT_SYMBOL)
        .ok_or_else(|| missing_symbol(RING_COUNT_SYMBOL, "how many rings are live"))?;
    let used = u64::from(read(c, u64::from(count_addr), 1)?[0]);
    // A count past the ceiling is the machine disagreeing with its own listing, not a bigger buffer.
    if used > l.max_ring_buffer {
        return Err(Refusal::window(
            "ringBufferUnreadable",
            format!(
                "Ring_Count reads {used} and this build's buffer holds {}, so the two do not agree \
                 and this window will not write into a buffer it cannot describe. Nothing was placed.",
                l.max_ring_buffer
            ),
            None,
        ));
    }
    if used == 0 {
        return Ok((0, Vec::new()));
    }
    let base = c
        .address_of(RING_BUFFER_SYMBOL)
        .ok_or_else(|| missing_symbol(RING_BUFFER_SYMBOL, "where the ring buffer starts"))?;
    let bytes = read(c, u64::from(base), used * l.entry_size)?;
    let mut taken = Vec::new();
    for i in 0..used {
        let at = (i * l.entry_size) as usize;
        if bytes[at + l.section_id_off as usize] == section_id {
            taken.push(u64::from(bytes[at + l.list_index_off as usize]));
        }
    }
    Ok((used, taken))
}

/// **Put a ring where the window was clicked.**
///
/// The whole order, and every step of it is a refusal that says which:
///
/// 1. the bounds, out of the listing's equate table ([`layout`]);
/// 2. the world pixel, through the same `emulator/object_at` join object placement uses
///    ([`crate::spawn::world_at`]) — one implementation, so a click cannot mean two places;
/// 3. the act's box ([`crate::spawn::in_act`]), because a ring outside it is culled as silently as an
///    object is;
/// 4. the tracked section the click landed in, or the third "cannot place";
/// 5. the section's real ring count, walked ([`census`]);
/// 6. a free index at or above it, or the second "cannot place";
/// 7. room in the buffer, or the first;
/// 8. the record, **then** the count, in that order.
///
/// **The record goes first and the count second, and that is not arbitrary.** The count is what every
/// reader of this buffer treats as the boundary between real records and stale bytes, so bumping it
/// before the record exists would publish a slot's previous contents as a ring for however long the
/// window between two writes lasts. The machine is paused, so today that window is empty; writing them
/// in the other order would be relying on that.
#[cfg(feature = "aether")]
pub fn place(c: &mut impl Caller, dot: (u16, u16)) -> Result<Placed, Refusal> {
    let l = layout(c)?;
    let world = crate::spawn::world_at(c, dot)?;
    crate::spawn::in_act(c, world)?;

    let tracked = sections(c, &l)?;
    let Some(section) = tracked
        .iter()
        .copied()
        .find(|s| s.contains(world, l.section_size))
    else {
        return Err(untracked(world, &tracked));
    };

    let cen = census(c, &l, section)?;
    // ⚑ **The buffer is asked about FIRST**, so the section's refusal is only ever reached on a buffer
    // that genuinely has room. Both can be full at once, and when they are, "the buffer is full" is the
    // one with the shorter way out: collect some rings or move away from a crowd of them, against moving
    // the camera into a whole different section of the level.
    if cen.buffer_used >= l.max_ring_buffer {
        return Err(buffer_full(cen.buffer_used, &l));
    }
    let Some(index) = cen.free_index(&l) else {
        return Err(cen.no_index(&l));
    };
    // Checked rather than cast. `Layout::check` has already refused a build whose ceiling could not fit,
    // so this is unreachable there and still not written as an `as`: a truncated index is the aliasing.
    let Ok(index_byte) = u8::try_from(index) else {
        return Err(Refusal::window(
            "ringRecordLayoutUnknown",
            format!(
                "the only free ring number in section {} is {index}, which does not fit in the one \
                 byte a ring record carries. Nothing was placed, because cutting it down would name a \
                 different ring.",
                section.id
            ),
            None,
        ));
    };
    // The world position is written whole. aeon's records are engine-space X/Y, which is the same flat
    // world-pixel space `emulator/object_at` reports and `Obj_Req_X/Y` take.
    let (Ok(x), Ok(y)) = (u16::try_from(world.0), u16::try_from(world.1)) else {
        return Err(Refusal::window(
            "outsideAct",
            format!(
                "world ({}, {}) does not fit in the two bytes a ring record carries for a position, \
                 so nothing was placed.",
                world.0, world.1
            ),
            None,
        ));
    };

    let base = c
        .address_of(RING_BUFFER_SYMBOL)
        .ok_or_else(|| missing_symbol(RING_BUFFER_SYMBOL, "where the ring buffer starts"))?;
    let count_addr = c
        .address_of(RING_COUNT_SYMBOL)
        .ok_or_else(|| missing_symbol(RING_COUNT_SYMBOL, "how many rings are live"))?;
    let slot = cen.buffer_used;
    let addr = l.record_addr(base, slot);
    write(c, addr, &l.record(x, y, section.id, index_byte))?;
    let used = slot + 1;
    write(c, u64::from(count_addr), &[used as u8])?;
    // ⚑ `RingBuffer_Add` maintains the high water mark and this stands in for `RingBuffer_Add`. The
    // player's own Objects panel reads it, so skipping it would make one of our readouts wrong about a
    // machine we had just written to. A build that does not publish the symbol is not a reason to refuse
    // a placement that has already happened, so the failure here is deliberately swallowed: the ring is
    // real either way and a second sentence about a debug statistic would be noise on top of the one the
    // person is reading.
    if let Some(hw) = c.address_of(RING_HIGH_WATER_SYMBOL) {
        if let Ok(b) = read(c, u64::from(hw), 1) {
            if u64::from(b[0]) < used {
                let _ = write(c, u64::from(hw), &[used as u8]);
            }
        }
    }

    Ok(Placed {
        world,
        section_id: section.id,
        list_index: index,
        real_rings: cen.real_rings,
        slot,
        addr,
        buffer_used: used,
        buffer_max: l.max_ring_buffer,
    })
}

/// `len` bytes from `addr`, refused rather than defaulted when the reply is not the shape it claims.
///
/// A short or unparseable reply is the one answer a measurement must never give quietly: every count in
/// this module is derived from these bytes, and a zero standing in for an unread byte is a ring placed at
/// index 0 of a section whose real rings start there.
#[cfg(feature = "aether")]
fn read(c: &mut impl Caller, addr: u64, len: u64) -> Result<Vec<u8>, Refusal> {
    let r = c.call(
        "emulator/read_memory",
        serde_json::json!({"addr": format!("0x{addr:08X}"), "len": len}),
    )?;
    let s = r["bytes"].as_str().unwrap_or_default();
    let s = s.strip_prefix("0x").unwrap_or(s);
    let mut out = Vec::with_capacity(len as usize);
    let raw: Vec<char> = s.chars().collect();
    for pair in raw.chunks(2) {
        if pair.len() != 2 {
            break;
        }
        match u8::from_str_radix(&pair.iter().collect::<String>(), 16) {
            Ok(b) => out.push(b),
            Err(_) => break,
        }
    }
    if out.len() as u64 != len {
        return Err(Refusal::local(format!(
            "the window asked for {len} byte(s) at {addr:#010X} and could not read that many back, \
             so nothing about the rings there has been measured and nothing was placed"
        )));
    }
    Ok(out)
}

/// `bytes` to `addr`, through the same served method a socket client would poke with.
#[cfg(feature = "aether")]
fn write(c: &mut impl Caller, addr: u64, bytes: &[u8]) -> Result<(), Refusal> {
    let hex: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
    c.call(
        "emulator/write_memory",
        serde_json::json!({"addr": format!("0x{addr:08X}"), "bytes": format!("0x{hex}")}),
    )?;
    Ok(())
}

/// The refusal for a RAM label the loaded listing does not name.
///
/// Its own sentence rather than [`Layout`]'s, because these are **labels** and those are **equates**, and
/// a build can publish one namespace and not the other. Telling a person their listing has no ring
/// equates when what it is actually missing is `Ring_Buffer` sends them to the wrong place.
fn missing_symbol(name: &str, what: &str) -> Refusal {
    Refusal::window(
        "ringSymbolsUnknown",
        format!(
            "the loaded listing does not name `{name}`, so this window cannot tell {what}, and \
             nothing was placed. This is a label rather than one of the ring layout's equates, so a \
             listing can carry the whole ring format and still be missing it."
        ),
        Some("load a listing for the build that is running, then click again".to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    // -----------------------------------------------------------------------------------------------
    // The fixtures. ⚑ Deliberately UNLIKE the live build in the dimensions under test.
    // -----------------------------------------------------------------------------------------------

    /// The real values, off `aeon/s4.debug.lst`, for the rows that assert this crate reads what is there.
    ///
    /// ⚑ It is **the sonic4 shape**, and the demo shape differs in exactly one number
    /// (`MAX_RING_BUFFER` is `$10` there), which is why [`demo_shape`] exists and why no row below is
    /// allowed to bake `$80`.
    fn s4_values() -> BTreeMap<String, u64> {
        let mut m = BTreeMap::new();
        for (k, v) in [
            ("MAX_RING_BUFFER", 0x80),
            ("MAX_LIST_ENTRIES", 0x80),
            ("MAX_TRACKED_SECTIONS", 0x04),
            ("RING_BUFFER_ENTRY_SIZE", 0x06),
            ("RING_ENTRY_X_OFFSET", 0x00),
            ("RING_ENTRY_Y_OFFSET", 0x02),
            ("RING_ENTRY_SECTION_ID_OFFSET", 0x04),
            ("RING_ENTRY_LIST_INDEX_OFFSET", 0x05),
            ("RING_LIST_ENTRY_SIZE", 0x04),
            ("RING_LIST_TERMINATOR", 0x00),
            ("EntityScanState_len", 0x1A),
            ("EntityScanState_ess_rom_ring_ptr", 0x04),
            ("EntityScanState_ess_section_id", 0x12),
            ("EntityScanState_ess_entry_idx", 0x13),
            ("EntityScanState_ess_origin_x", 0x10),
            ("EntityScanState_ess_origin_y", 0x14),
            ("SECTION_SIZE", 0x800),
            ("SEC_VOID", 0xFF),
        ] {
            m.insert(k.to_string(), v);
        }
        m
    }

    fn s4() -> Layout {
        Layout::from_values(&s4_values()).expect("the real sonic4 numbers describe a real record")
    }

    /// The same listing with the one number that differs between this engine's two games.
    fn demo_shape() -> Layout {
        let mut v = s4_values();
        v.insert("MAX_RING_BUFFER".to_string(), 0x10);
        Layout::from_values(&v).unwrap()
    }

    fn section(id: u8, origin: (u16, u16), ptr: u32) -> Section {
        Section {
            entry: 0,
            id,
            origin,
            rom_ring_ptr: ptr,
        }
    }

    /// Every sentence this module can put in front of a person, over every arm, for the style sweeps.
    ///
    /// **Assembled from the constructors** rather than from a list of literals, so a refusal added later
    /// is swept without anybody remembering to add it here.
    fn every_sentence() -> Vec<String> {
        let l = s4();
        let s = section(3, (2048, 0), 0x0001_0000);
        let full = Census {
            section: s,
            real_rings: 0x80,
            taken: vec![1, 2],
            buffer_used: 4,
        };
        let empty = Census {
            section: s,
            real_rings: 0,
            taken: Vec::new(),
            buffer_used: 0,
        };
        let mut v = vec![
            TEMPORARY.to_string(),
            full.no_index(&l).message,
            empty.no_index(&l).message,
            buffer_full(0x80, &l).message,
            untracked((99, 99), &[s]).message,
            untracked((99, 99), &[]).message,
            missing_symbol(RING_BUFFER_SYMBOL, "where the ring buffer starts").message,
            Layout::from_values(&BTreeMap::new()).unwrap_err().message,
            Placed {
                world: (100, 200),
                section_id: 3,
                list_index: 9,
                real_rings: 9,
                slot: 2,
                addr: 0x00FF_AF3C,
                buffer_used: 3,
                buffer_max: 0x80,
            }
            .terminal(),
        ];
        // The layout-inconsistency arm, which is a whole family of sentences of its own.
        let mut bad = s4_values();
        bad.insert("RING_ENTRY_LIST_INDEX_OFFSET".to_string(), 0x40);
        v.push(Layout::from_values(&bad).unwrap_err().message);
        let mut wide = s4_values();
        wide.insert("MAX_LIST_ENTRIES".to_string(), 0x400);
        v.push(Layout::from_values(&wide).unwrap_err().message);
        // And every remedy, which is text a person reads too.
        let mut remedies: Vec<String> = Vec::new();
        for r in [
            full.no_index(&l),
            buffer_full(0, &l),
            untracked((0, 0), &[]),
            missing_symbol("Ring_Count", "how many rings are live"),
            Layout::from_values(&BTreeMap::new()).unwrap_err(),
        ] {
            remedies.extend(r.remedy.clone());
        }
        v.extend(remedies);
        v
    }

    // -----------------------------------------------------------------------------------------------
    // The bounds are read, and a missing one is a sentence
    // -----------------------------------------------------------------------------------------------

    /// ⚑ **The one number that differs between this engine's two games is carried, not assumed.**
    ///
    /// This is the guard the handoff asks for by name: *"`MAX_RING_BUFFER` is `$80` on sonic4 and `$10`
    /// on demo. Read it, never type it."* A picker that hard-coded either would be silently wrong on the
    /// other game, and both shapes are real builds on this box.
    #[test]
    fn the_buffer_ceiling_is_the_listings_and_the_two_games_disagree_about_it() {
        assert_eq!(s4().max_ring_buffer, 0x80);
        assert_eq!(demo_shape().max_ring_buffer, 0x10);
        assert_ne!(
            s4().max_ring_buffer,
            demo_shape().max_ring_buffer,
            "if these ever agree this row proves nothing, and the fixture must be re-derived from the \
             two listings rather than left agreeing"
        );
        // The refusal for a full buffer quotes the ceiling it was decided from, so the two games get two
        // different sentences from one implementation.
        let s4_full = buffer_full(0x80, &s4()).message;
        let demo_full = buffer_full(0x10, &demo_shape()).message;
        assert!(s4_full.contains("128") && !s4_full.contains("16 records"));
        assert!(demo_full.contains("16"));
        assert_ne!(s4_full, demo_full);
    }

    /// **A bound the listing does not publish is a stated refusal that names every missing one.**
    #[test]
    fn an_absent_bound_is_named_and_never_defaulted() {
        let e = Layout::from_values(&BTreeMap::new()).expect_err("an empty listing cannot answer");
        assert_eq!(e.reason.as_deref(), Some("ringBoundsUnknown"));
        for name in EQUATES {
            assert!(
                e.message.contains(name),
                "every missing name must be in the one sentence, or a person discovers them one \
                 gesture at a time: {name} is not in {:?}",
                e.message
            );
        }

        // One missing name is the same finding and names only that one.
        let mut one = s4_values();
        one.remove("MAX_RING_BUFFER");
        let e = Layout::from_values(&one).expect_err("one absent bound is still no measurement");
        assert!(e.message.contains("MAX_RING_BUFFER"));
        assert!(
            !e.message.contains("MAX_LIST_ENTRIES"),
            "a name that IS published must not be reported missing: {:?}",
            e.message
        );
    }

    /// **A record whose published offsets do not fit inside its published size is refused**, and that is
    /// a different finding from a missing name.
    #[test]
    fn a_record_that_does_not_fit_its_own_size_is_refused_rather_than_written_past() {
        let mut v = s4_values();
        v.insert("RING_ENTRY_Y_OFFSET".to_string(), 0x05);
        let e =
            Layout::from_values(&v).expect_err("a two-byte field at offset 5 of a 6-byte record");
        assert_eq!(e.reason.as_deref(), Some("ringRecordLayoutUnknown"));
        assert!(e.message.contains("RING_ENTRY_Y_OFFSET"), "{:?}", e.message);

        // A stride of zero would make every walk in this module spin on one entry forever.
        let mut v = s4_values();
        v.insert("RING_LIST_ENTRY_SIZE".to_string(), 0);
        assert_eq!(
            Layout::from_values(&v).unwrap_err().reason.as_deref(),
            Some("ringRecordLayoutUnknown")
        );

        // ⚑ And a ceiling past what the record's one-byte index field can carry. A build that raised
        // MAX_LIST_ENTRIES would otherwise get its index cut down into a real ring's collected bit,
        // which is the exact damage this module exists to prevent, arriving through the bound meant to
        // prevent it.
        let mut v = s4_values();
        v.insert("MAX_LIST_ENTRIES".to_string(), 0x400);
        let e = Layout::from_values(&v).unwrap_err();
        assert!(e.message.contains("one byte"), "{:?}", e.message);
    }

    /// **The record is composed at the listing's offsets**, so a build that moved a field moves the byte.
    #[test]
    fn the_record_puts_each_field_where_the_listing_says_and_not_where_it_is_written_here() {
        let l = s4();
        let r = l.record(0x1234, 0x5678, 0x03, 0x09);
        assert_eq!(r.len(), l.entry_size as usize);
        assert_eq!(r, vec![0x12, 0x34, 0x56, 0x78, 0x03, 0x09]);

        // Swap the two byte fields in the listing and the composed record follows, with no line here
        // changing. A `vec![x_hi, x_lo, y_hi, y_lo, section, index]` would keep passing the row above
        // and fail this one, which is why both exist.
        let mut v = s4_values();
        v.insert("RING_ENTRY_SECTION_ID_OFFSET".to_string(), 0x05);
        v.insert("RING_ENTRY_LIST_INDEX_OFFSET".to_string(), 0x04);
        let swapped = Layout::from_values(&v).unwrap();
        assert_eq!(
            swapped.record(0x1234, 0x5678, 0x03, 0x09),
            vec![0x12, 0x34, 0x56, 0x78, 0x09, 0x03]
        );
    }

    // -----------------------------------------------------------------------------------------------
    // Rule 2, which is the destructive one
    // -----------------------------------------------------------------------------------------------

    /// ⚑ **The chosen index is never below the section's real ring count.**
    ///
    /// This is the whole rule, as a property rather than as a case: for every count from an empty
    /// section to a full one, whatever this picks is at or above that count and below the ceiling. An
    /// index below it marks a *different, real* ring collected and stops it appearing, and the damage
    /// lands on another object at another time with nothing pointing back here.
    #[test]
    fn no_index_this_picks_can_alias_a_real_rings_collected_bit() {
        let l = s4();
        let s = section(3, (0, 0), 0x0001_0000);
        for real in 0..=l.max_list_entries {
            let c = Census {
                section: s,
                real_rings: real,
                taken: Vec::new(),
                buffer_used: 0,
            };
            match c.free_index(&l) {
                Some(i) => {
                    assert!(
                        i >= real,
                        "index {i} is below section {}'s {real} real rings, which is the aliasing",
                        s.id
                    );
                    assert!(i < l.max_list_entries, "index {i} is past the ceiling");
                    assert_eq!(i, real, "the lowest safe index is the count itself");
                }
                None => assert_eq!(
                    real, l.max_list_entries,
                    "the only section with no free index is one whose real rings fill the range"
                ),
            }
        }
    }

    /// **A section already holding indexes this window used does not hand out one of them twice**, and
    /// the count is still the floor.
    #[test]
    fn a_second_placement_in_one_section_takes_the_next_index_and_not_the_first_one_again() {
        let l = s4();
        let s = section(3, (0, 0), 0x0001_0000);
        let c = Census {
            section: s,
            real_rings: 9,
            taken: vec![9, 10, 12],
            buffer_used: 3,
        };
        assert_eq!(c.free_index(&l), Some(11), "the gap is used before the top");

        // Indexes taken in a DIFFERENT section are not this section's business: the collected mask is
        // per section, so the same number in two sections is two different bits.
        let other = Census {
            section: section(4, (2048, 0), 0x0002_0000),
            real_rings: 9,
            taken: Vec::new(),
            buffer_used: 3,
        };
        assert_eq!(other.free_index(&l), Some(9));
    }

    /// ⚑ **The two "cannot place" states are two different sentences, and neither is a greyed-out box.**
    ///
    /// A full buffer and a full index space are unrelated: the buffer can be nearly empty while one
    /// section has no number left, and the buffer can be full while every section has numbers to spare.
    /// Reporting either as the other sends a person to fix the wrong thing.
    #[test]
    fn a_full_buffer_and_a_full_index_space_are_told_apart_in_words() {
        let l = s4();
        let s = section(3, (0, 0), 0x0001_0000);

        // Index space exhausted while the buffer is nearly empty.
        let cramped = Census {
            section: s,
            real_rings: l.max_list_entries,
            taken: Vec::new(),
            buffer_used: 4,
        };
        assert_eq!(cramped.free_index(&l), None);
        let a = cramped.no_index(&l);
        assert_eq!(a.reason.as_deref(), Some("ringSectionFull"));
        assert!(
            a.message.contains("is not the problem") && a.message.contains("4 of 128"),
            "the section refusal must say the buffer is fine, with the numbers: {:?}",
            a.message
        );

        // Buffer exhausted while the section has room.
        let roomy = Census {
            section: s,
            real_rings: 2,
            taken: Vec::new(),
            buffer_used: l.max_ring_buffer,
        };
        assert_eq!(
            roomy.free_index(&l),
            Some(2),
            "the section is not the limit"
        );
        let b = buffer_full(roomy.buffer_used, &l);
        assert_eq!(b.reason.as_deref(), Some("ringBufferFull"));
        assert!(
            b.message.contains("not the same as the section"),
            "the buffer refusal must say it is not the section's: {:?}",
            b.message
        );
        assert_ne!(a.message, b.message);
        assert_ne!(a.reason, b.reason);
        assert_ne!(a.remedy, b.remedy, "two states, two next actions");
    }

    /// ⚑ **When BOTH limits are hit at once, no sentence claims the other one is fine.**
    ///
    /// The defect this closes was in this module's own first draft: the section refusal said *"the ring
    /// buffer is not the problem here"* unconditionally, which is a false statement in exactly the case
    /// where a person most needs a true one. The whole reason these two states are told apart is that a
    /// reader takes one sentence and goes and fixes the thing it names; a sentence that clears the
    /// buffer while the buffer is full sends them to fix something that is not broken and leaves the
    /// thing that is.
    #[test]
    fn a_refusal_never_clears_the_other_limit_when_both_limits_are_hit() {
        let l = s4();
        let s = section(3, (0, 0), 0x0001_0000);
        let both = Census {
            section: s,
            real_rings: l.max_list_entries,
            taken: Vec::new(),
            buffer_used: l.max_ring_buffer,
        };
        assert_eq!(both.free_index(&l), None, "the section has nothing to give");
        let m = both.no_index(&l).message;
        assert!(
            !m.contains("not the problem"),
            "the buffer IS full, and this sentence must not say otherwise: {m:?}"
        );
        assert!(
            m.contains("full as well") && m.contains("both limits"),
            "when both limits are in the way the sentence must say so: {m:?}"
        );

        // And the ordinary case still clears the buffer, because there it is true.
        let only_section = Census {
            buffer_used: 4,
            ..both
        };
        assert!(only_section
            .no_index(&l)
            .message
            .contains("not the problem"));
    }

    /// **The third state the handoff does not name**: a click in a section the camera is not tracking.
    #[test]
    fn a_click_outside_every_tracked_section_is_its_own_stated_refusal() {
        let l = s4();
        let here = section(3, (2048, 0), 0x0001_0000);
        let there = section(4, (4096, 0), 0x0002_0000);
        let all = [here, there];

        assert!(
            here.contains((2048, 0), l.section_size),
            "the origin is inside"
        );
        assert!(
            here.contains((4095, 2047), l.section_size),
            "the last pixel of the square is inside"
        );
        assert!(
            !here.contains((4096, 0), l.section_size),
            "the square is half open, so the next section's first column is not this one's"
        );
        assert!(!here.contains((2048, 2048), l.section_size));

        let hit = all
            .iter()
            .find(|s| s.contains((4100, 10), l.section_size))
            .expect("that pixel is in the second section");
        assert_eq!(hit.id, 4);

        assert!(all.iter().all(|s| !s.contains((100, 100), l.section_size)));
        let e = untracked((100, 100), &all);
        assert_eq!(e.reason.as_deref(), Some("ringSectionUntracked"));
        assert!(
            e.message.contains("section 3") && e.message.contains("section 4"),
            "the refusal names what IS tracked, so the way out is visible: {:?}",
            e.message
        );
        // Nothing tracked at all is a different clause, not an empty list rendered as one.
        let none = untracked((100, 100), &[]);
        assert!(none.message.contains("tracking none"), "{:?}", none.message);
        assert_ne!(e.message, none.message);
    }

    /// ⚑ **A null ring-list pointer is UNMEASURABLE, not a count of zero.**
    ///
    /// It is the boot-cleared scan slot and the genuinely empty section wearing one face, and reading it
    /// as zero would place a ring at index 0 of section 0 the moment an act loaded.
    #[test]
    fn a_section_with_no_list_pointer_cannot_be_measured() {
        assert!(!section(0, (0, 0), 0).has_list());
        assert!(section(0, (0, 0), 0x0001_0000).has_list());
    }

    // -----------------------------------------------------------------------------------------------
    // Rule 1, which is the one a reader has to be told
    // -----------------------------------------------------------------------------------------------

    /// **The vanishing is said in plain words, and it is the same words everywhere.**
    ///
    /// A condition of the feature rather than a polish item: without it a ring that disappears on the
    /// first camera move reads as a broken tool. The success line carries the constant rather than a
    /// paraphrase, so the panel and the terminal cannot end up describing two different designs.
    #[test]
    fn the_placement_line_says_the_ring_is_temporary_in_the_standing_statement_words() {
        let p = Placed {
            world: (100, 200),
            section_id: 3,
            list_index: 9,
            real_rings: 9,
            slot: 2,
            addr: 0x00FF_AF3C,
            buffer_used: 3,
            buffer_max: 0x80,
        };
        let t = p.terminal();
        assert!(
            t.contains(TEMPORARY),
            "the success line must carry the standing statement whole, not a second wording of it: \
             {t:?}"
        );
        assert!(
            TEMPORARY.contains("TEMPORARY") && TEMPORARY.contains("camera"),
            "the statement must say what happens and when, in the words a person reads"
        );
        assert!(
            !TEMPORARY.contains("EntityWindow") && !TEMPORARY.contains("despawn"),
            "the person at this window is not reading the engine's source"
        );
        // And the numbers that make the placement checkable are on the line.
        assert!(t.contains("section 3") && t.contains("(100, 200)"));
        assert!(
            t.contains("3 of 128"),
            "the buffer's occupancy is what tells a person the next click may be refused: {t:?}"
        );
        assert!(
            p.toast().contains("TEMPORARY"),
            "the short form drops the detail, never the rule"
        );
    }

    // -----------------------------------------------------------------------------------------------
    // The style rules every sentence in this crate is held to
    // -----------------------------------------------------------------------------------------------

    /// **P10** (no em or en dashes), **P9** (no specification citations) and **P2** (no column drawn
    /// inside a string), over every sentence this module can produce.
    #[test]
    fn nothing_this_module_says_carries_a_dash_or_cites_the_specification_or_pads_a_column() {
        let all = every_sentence();
        assert!(
            all.len() >= 15,
            "the sweep must reach every arm: {}",
            all.len()
        );
        for s in all {
            for bad in ['\u{2014}', '\u{2013}'] {
                assert!(
                    !s.contains(bad),
                    "user-facing text carries {bad:?}, which the owner's 2026-09-05 ruling bars: {s:?}"
                );
            }
            assert!(
                !s.contains('\u{a7}') && !s.contains("protocol.md"),
                "the person at this window is not holding the specification: {s:?}"
            );
            assert!(
                !s.contains("  ") && !s.contains('\t'),
                "a run of spaces is a column drawn inside a string: {s:?}"
            );
            assert!(s.len() > 20, "every sentence must say something: {s:?}");
        }
    }

    // -----------------------------------------------------------------------------------------------
    // The wire
    // -----------------------------------------------------------------------------------------------

    /// A [`Caller`] over a scripted machine: a byte map for reads, a log of writes, and a listing.
    #[cfg(feature = "aether")]
    struct Fake {
        equates: BTreeMap<String, u64>,
        symbols: BTreeMap<String, u32>,
        mem: BTreeMap<u64, u8>,
        world: (u64, u64),
        wrote: Vec<(u64, Vec<u8>)>,
        asked: Vec<String>,
    }

    #[cfg(feature = "aether")]
    impl Fake {
        fn poke(&mut self, addr: u64, bytes: &[u8]) {
            for (i, b) in bytes.iter().enumerate() {
                self.mem.insert(addr + i as u64, *b);
            }
        }
    }

    #[cfg(feature = "aether")]
    impl Caller for Fake {
        fn call(
            &mut self,
            method: &str,
            params: serde_json::Value,
        ) -> Result<serde_json::Value, Refusal> {
            self.asked.push(method.to_string());
            match method {
                // ⚑ The equate door. A `lookup_symbol` here would be the mistake this module's header is
                // about, and the fake refuses it below rather than answering, so a regression to the
                // wrong door fails loudly instead of reading as an absent name.
                "emulator/lookup_equate" => {
                    let name = params["name"].as_str().unwrap_or_default();
                    match self.equates.get(name) {
                        Some(v) => Ok(serde_json::json!({"name": name, "value": v})),
                        None => Err(Refusal::local(format!("no equate named {name}"))),
                    }
                }
                "emulator/object_at" => Ok(serde_json::json!({
                    "worldSource": "camera",
                    "world": {"x": self.world.0, "y": self.world.1}
                })),
                "emulator/read_memory" => {
                    let addr = u64::from_str_radix(
                        params["addr"].as_str().unwrap().trim_start_matches("0x"),
                        16,
                    )
                    .unwrap();
                    let len = params["len"].as_u64().unwrap();
                    let hex: String = (0..len)
                        .map(|i| format!("{:02X}", self.mem.get(&(addr + i)).copied().unwrap_or(0)))
                        .collect();
                    Ok(serde_json::json!({"bytes": format!("0x{hex}")}))
                }
                "emulator/write_memory" => {
                    let addr = u64::from_str_radix(
                        params["addr"].as_str().unwrap().trim_start_matches("0x"),
                        16,
                    )
                    .unwrap();
                    let s = params["bytes"].as_str().unwrap().trim_start_matches("0x");
                    let bytes: Vec<u8> = s
                        .as_bytes()
                        .chunks(2)
                        .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
                        .collect();
                    self.poke(addr, &bytes);
                    self.wrote.push((addr, bytes));
                    Ok(serde_json::json!({"addr": params["addr"], "len": 1}))
                }
                other => panic!("unscripted call to {other}"),
            }
        }

        fn address_of(&mut self, symbol: &str) -> Option<u32> {
            self.symbols.get(symbol).copied()
        }
    }

    /// A machine with `Level_Width`/`Level_Height` set, one tracked section holding `rings` real rings,
    /// and an empty ring buffer.
    #[cfg(feature = "aether")]
    fn machine(rings: u64, buffer_used: u8) -> Fake {
        const BUF: u32 = 0x00FF_AF30;
        const COUNT: u32 = 0x00FF_B230;
        const HIGH: u32 = 0x00FF_B231;
        const SCAN: u32 = 0x00FF_B234;
        const LIST: u32 = 0x0002_0000;
        const LW: u32 = 0x00FF_BABE;
        const LH: u32 = 0x00FF_BAC0;
        let l = s4();
        let mut f = Fake {
            equates: s4_values(),
            symbols: BTreeMap::from([
                (RING_BUFFER_SYMBOL.to_string(), BUF),
                (RING_COUNT_SYMBOL.to_string(), COUNT),
                (RING_HIGH_WATER_SYMBOL.to_string(), HIGH),
                (SCAN_STATE_SYMBOL.to_string(), SCAN),
                ("Level_Width".to_string(), LW),
                ("Level_Height".to_string(), LH),
            ]),
            mem: BTreeMap::new(),
            world: (2100, 300),
            wrote: Vec::new(),
            asked: Vec::new(),
        };
        f.poke(u64::from(LW), &0x1800u16.to_be_bytes());
        f.poke(u64::from(LH), &0x1800u16.to_be_bytes());
        // Scan slot 0 tracks section 3 at (2048, 0); the other three are void.
        let s0 = u64::from(SCAN);
        f.poke(s0 + l.scan_rom_ring_ptr_off, &LIST.to_be_bytes());
        f.poke(s0 + l.scan_section_id_off, &[3]);
        f.poke(s0 + l.scan_entry_idx_off, &[0]);
        f.poke(s0 + l.scan_origin_x_off, &2048u16.to_be_bytes());
        f.poke(s0 + l.scan_origin_y_off, &0u16.to_be_bytes());
        for i in 1..l.max_tracked_sections {
            f.poke(s0 + i * l.scan_len + l.scan_section_id_off, &[0xFF]);
        }
        // `rings` real entries, then the terminator. Each is a non-zero long so the walk sees it.
        for i in 0..rings {
            f.poke(
                u64::from(LIST) + i * l.list_entry_size,
                &[0x01, (i as u8) | 1, 0x00, 0x40],
            );
        }
        f.poke(u64::from(LIST) + rings * l.list_entry_size, &[0, 0, 0, 0]);
        f.poke(u64::from(COUNT), &[buffer_used]);
        f
    }

    /// ⚑ **The whole placement, end to end, on a machine with real rings in the section.**
    ///
    /// The load-bearing assertion is the index: the section has nine real rings and the record must
    /// carry nine, not zero. A placement at zero would look identical on screen and would mark the
    /// level's first ring collected the moment this one was picked up.
    #[cfg(feature = "aether")]
    #[test]
    fn a_placed_ring_lands_above_the_sections_real_rings_and_the_count_is_bumped_after_the_record()
    {
        let l = s4();
        let mut f = machine(9, 0);
        let p = place(&mut f, (10, 20)).expect("a tracked section with room places a ring");

        assert_eq!(p.real_rings, 9, "the count was walked, not assumed");
        assert_eq!(
            p.list_index, 9,
            "the index must clear the section's real rings, or collecting this ring marks one of \
             them collected"
        );
        assert_eq!(p.section_id, 3);
        assert_eq!(p.world, (2100, 300));
        assert_eq!(p.slot, 0);
        assert_eq!(p.buffer_used, 1);
        assert_eq!(p.buffer_max, l.max_ring_buffer);

        // ⚑ THE ORDER. The record exists before the count claims it does.
        let record = f
            .wrote
            .iter()
            .position(|(a, _)| *a == u64::from(0x00FF_AF30u32))
            .expect("the record was written");
        let count = f
            .wrote
            .iter()
            .position(|(a, _)| *a == u64::from(0x00FF_B230u32))
            .expect("the count was written");
        assert!(
            record < count,
            "the count is what every reader treats as the boundary between records and stale bytes, \
             so it must be raised only after the record it is counting exists"
        );
        assert_eq!(f.wrote[record].1, l.record(2100, 300, 3, 9));
        assert_eq!(f.wrote[count].1, vec![1]);

        // The high water mark follows, because our own Objects panel reads it.
        let hw = f
            .wrote
            .iter()
            .find(|(a, _)| *a == u64::from(0x00FF_B231u32))
            .expect("RingBuffer_Add maintains the high water mark and so does this");
        assert_eq!(hw.1, vec![1]);

        // ⚑ And the bounds came through the equate door. A `lookup_symbol` here refuses in a way that
        // reads exactly like the name not existing, which is the trap this module's header names.
        assert!(
            f.asked.iter().any(|m| m == "emulator/lookup_equate"),
            "the bounds must be read, and through the equate door"
        );
        assert!(
            !f.asked.iter().any(|m| m == "emulator/lookup_symbol"),
            "an equate asked for through the symbol door refuses as if it were absent: {:?}",
            f.asked
        );
    }

    /// **A section with no real rings still places, at index zero**, because zero is at or above zero.
    ///
    /// The fixture reaches this deliberately: with only the nine-ring case, an implementation that
    /// returned `real_rings.max(1)` or that refused an empty list would look correct.
    #[cfg(feature = "aether")]
    #[test]
    fn a_section_whose_list_is_empty_places_at_index_zero() {
        let mut f = machine(0, 0);
        let p =
            place(&mut f, (10, 20)).expect("an empty list is a measurement of zero, not a refusal");
        assert_eq!(p.real_rings, 0);
        assert_eq!(p.list_index, 0);
    }

    /// **A full buffer refuses with the buffer's sentence**, on a machine whose section has room.
    #[cfg(feature = "aether")]
    #[test]
    fn a_full_buffer_refuses_and_says_it_is_the_buffer_and_not_the_section() {
        let l = s4();
        let mut f = machine(9, l.max_ring_buffer as u8);
        let e = place(&mut f, (10, 20)).expect_err("there is no slot to write into");
        assert_eq!(e.reason.as_deref(), Some("ringBufferFull"));
        assert!(f.wrote.is_empty(), "a refusal must write nothing");

        // ⚑ **Both limits at once, over the wire**: the buffer is asked about first, because its way out
        // is the shorter of the two (collect some rings, against move the camera to another section of
        // the level). Either answer would be true; this is the one a person can act on where they stand.
        let mut f = machine(l.max_list_entries, l.max_ring_buffer as u8);
        let e = place(&mut f, (10, 20)).expect_err("neither limit has anything to give");
        assert_eq!(
            e.reason.as_deref(),
            Some("ringBufferFull"),
            "with both full, the buffer is the one reported"
        );
        assert!(f.wrote.is_empty());
    }

    /// **A section whose index space is exhausted refuses with the SECTION's sentence**, on a machine
    /// whose buffer is nearly empty. The other half of the pair, reached through the wire rather than
    /// constructed.
    #[cfg(feature = "aether")]
    #[test]
    fn a_section_with_no_index_left_refuses_while_the_buffer_still_has_room() {
        let l = s4();
        let mut f = machine(l.max_list_entries, 2);
        let e = place(&mut f, (10, 20)).expect_err("every index belongs to a real ring");
        assert_eq!(e.reason.as_deref(), Some("ringSectionFull"));
        assert!(e.message.contains("is not the problem"), "{:?}", e.message);
        assert!(f.wrote.is_empty(), "a refusal must write nothing");
    }

    /// **A click outside every tracked section refuses and writes nothing.**
    #[cfg(feature = "aether")]
    #[test]
    fn a_click_in_an_untracked_section_writes_nothing() {
        let mut f = machine(9, 0);
        // Inside the act's 0x1800 box and outside section 3's square at (2048, 0).
        f.world = (100, 100);
        let e = place(&mut f, (10, 20)).expect_err("no tracked section holds that pixel");
        assert_eq!(e.reason.as_deref(), Some("ringSectionUntracked"));
        assert!(f.wrote.is_empty());
    }

    /// ⚑ **An unterminated list is unmeasurable and nothing is placed.**
    ///
    /// A pointer into the wrong part of the cartridge yields a plausible sequence that never ends.
    /// Stopping at the ceiling and calling that the count would hand the ceiling back as a measurement,
    /// and every index above it is out of range, so the placement would be refused for the wrong reason.
    #[cfg(feature = "aether")]
    #[test]
    fn a_ring_list_that_never_terminates_is_refused_as_unmeasurable() {
        let l = s4();
        let mut f = machine(9, 0);
        for i in 0..=l.max_list_entries {
            f.poke(
                0x0002_0000 + i * l.list_entry_size,
                &[0x01, 0x01, 0x00, 0x40],
            );
        }
        let e = place(&mut f, (10, 20)).expect_err("a list with no end cannot be counted");
        assert_eq!(e.reason.as_deref(), Some("ringListUnmeasurable"));
        assert!(f.wrote.is_empty());
    }

    /// **A build whose listing has no ring equates refuses before it reads anything else.**
    #[cfg(feature = "aether")]
    #[test]
    fn a_listing_with_no_ring_layout_refuses_and_never_reaches_the_machine() {
        let mut f = machine(9, 0);
        f.equates.clear();
        let e = place(&mut f, (10, 20)).expect_err("no bounds, no placement");
        assert_eq!(e.reason.as_deref(), Some("ringBoundsUnknown"));
        assert!(f.wrote.is_empty());
        assert!(
            !f.asked.iter().any(|m| m == "emulator/read_memory"),
            "the bounds gate is first, so a build that cannot answer is not read from"
        );
    }

    /// **The demo shape places too**, and its refusal quotes its own ceiling.
    ///
    /// The vacuity this closes: every row above runs on the sonic4 numbers, so a hard-coded `$80`
    /// anywhere in the walk would pass all of them.
    #[cfg(feature = "aether")]
    #[test]
    fn the_smaller_game_places_against_its_own_ceiling_and_not_the_bigger_ones() {
        let demo = demo_shape();
        let mut f = machine(9, 0);
        f.equates
            .insert("MAX_RING_BUFFER".to_string(), demo.max_ring_buffer);
        let p = place(&mut f, (10, 20)).expect("the smaller buffer still has room at zero");
        assert_eq!(
            p.buffer_max, 0x10,
            "the ceiling reported is the one this build published"
        );

        // And a count that is full for the demo, which is nowhere near full for sonic4.
        let mut f = machine(9, 0x10);
        f.equates.insert("MAX_RING_BUFFER".to_string(), 0x10);
        let e = place(&mut f, (10, 20)).expect_err("16 records fill the demo's buffer");
        assert_eq!(e.reason.as_deref(), Some("ringBufferFull"));
        assert!(
            e.message.contains("16"),
            "the sentence must quote this build's ceiling: {:?}",
            e.message
        );

        // The same machine on the sonic4 ceiling places, which is what makes the row above a measurement
        // of the ceiling rather than of the count.
        let mut f = machine(9, 0x10);
        place(&mut f, (10, 20)).expect("16 of 128 is not full");
    }

    /// **A click outside the act is refused by the act gate**, which is the same gate object placement
    /// runs and not a second copy of it.
    #[cfg(feature = "aether")]
    #[test]
    fn a_ring_outside_the_act_is_refused_by_the_shared_act_gate() {
        let mut f = machine(9, 0);
        f.world = (0x2000, 0x10);
        let e = place(&mut f, (10, 20)).expect_err("0x2000 is past the act's 0x1800");
        assert_eq!(
            e.reason.as_deref(),
            Some("outsideAct"),
            "the reason is the shared gate's own, not one invented here"
        );
        assert!(f.wrote.is_empty());
    }
}
