//! **Watch a named RAM byte and say what it became.** A configured symbol, an ordered list of labels,
//! and a toast whenever the value changes — nothing armed, no client, no bus call.
//!
//! # Why this exists
//!
//! Aeon has a debug hotkey that cycles twenty background-effect scenes live. It works; what it cannot do
//! is *say* which scene is on screen, because there is no debug-text path in a running frame and building
//! a font renderer into the ROM costs far more than the feature is worth. The owner's ask, verbatim:
//!
//! > *"do you think we could add something to either the emulator or somewhere else to say in text what
//! > kind of scene we're switching to? just so I know? Like 1/20 - Fire BG or something."*
//!
//! The player already owns a font, an overlay and a `.lst` reader. So the readout lives here.
//!
//! # Why the names are DATA and not Rust
//!
//! The twenty scene names are *aeon's vocabulary for aeon's feature*. Hardcoding them here would ship
//! another tool's model inside this emulator: the array goes stale the day aeon renames a scene, nothing
//! in this repo's gates would notice, and the next lane that wants the same readout gets nothing. This
//! repo already refuses to name another tool's tile-blob slots for exactly that reason. A config entry
//! naming a symbol plus a label list is barely more code than the array and strictly less coupling.
//!
//! It stops there on purpose. There is no expression language, no second watch kind, no format template.
//! One symbol, a list of labels, a toast on change.
//!
//! # It reads a BYTE, and that is a deliberate correction
//!
//! The parcel brief said "a RAM word". The addressed symbol is not one: `games/sonic4/config/ram.emp`
//! declares `Debug_Scene_Index: u8`, the game touches it with `move.b`, and in the debug shape it lands
//! on an **odd** address — a word read there would splice in the neighbouring byte and report a number
//! the game never held. Twenty labels cannot overflow a byte either. Widths beyond `u8` are a knob this
//! does not have; see the handoff doc's follow-up note.
//!
//! # It cannot perturb the machine
//!
//! [`SymbolWatch::poll`] takes `&[u8]` — the borrow [`oracle_core::system::System::ram`] hands out. No
//! bus cycle is issued, no clock advances, no VDP port is touched, and the argument is immutable at the
//! type level. There is no path from here back into the machine.

use oracle_core::symbols::{AddrSpace, SymbolTable};
use oracle_core::system::RAM_SIZE;

/// One configured watch, exactly as the config file spells it: a symbol name and an ordered label list.
/// Pure data — it names nothing this build knows about and resolves nothing.
///
/// A label may be empty. That is *positional* silence, not a shorter list: `a, , c` means index 1 has no
/// name while index 2 is still `c`, which is how a list with a gap in it survives a round trip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchSpec {
    /// The symbol to look up, in either the raw (`$module$Parent$local`) or demangled spelling —
    /// [`SymbolTable::address_of`] tries both.
    pub symbol: String,
    /// Label per value, index 0 upward. May be empty (watch reports raw numbers) or contain empty
    /// entries (that value reports `<no label>`).
    pub labels: Vec<String>,
}

/// Parse one `symbol_watch` value: `Name: first, second, third`.
///
/// * `Ok(Some(spec))` — a usable watch.
/// * `Ok(None)` — the value was blank. That is how the key round-trips when nothing is configured, so it
///   must not be an error and must not warn.
/// * `Err(reason)` — a value that names no symbol. The caller turns it into the per-key warning the
///   config file's failure model already uses for a bad value under a known key.
///
/// The colon is optional: `Name` alone is a legitimate "just tell me the number" watch. Everything after
/// the **first** colon is the label list, so a label may contain a colon; a label may not contain a comma,
/// which is the one thing this grammar cannot spell.
pub fn parse_spec(value: &str) -> Result<Option<WatchSpec>, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let (symbol, labels) = match value.split_once(':') {
        Some((s, l)) => (s.trim(), l.trim()),
        None => (value, ""),
    };
    if symbol.is_empty() {
        return Err(format!("want `Symbol: label, label`, got `{value}`"));
    }
    // A trailing-but-empty label list is *no* labels, not one blank one — `"".split(',')` yields a single
    // empty item, which would make `Name:` claim a one-entry list and report `1/1` forever.
    let labels = if labels.is_empty() {
        Vec::new()
    } else {
        labels.split(',').map(|l| l.trim().to_string()).collect()
    };
    Ok(Some(WatchSpec {
        symbol: symbol.to_string(),
        labels,
    }))
}

/// The inverse of [`parse_spec`], so a hand-edited file survives an in-app save unchanged.
pub fn format_spec(spec: &WatchSpec) -> String {
    if spec.labels.is_empty() {
        spec.symbol.clone()
    } else {
        format!("{}: {}", spec.symbol, spec.labels.join(", "))
    }
}

/// One watch that resolved to a real RAM byte, with the last value seen there.
#[derive(Debug)]
struct Armed {
    spec: WatchSpec,
    /// Index into the 64 KiB work-RAM slice, mirrored exactly as the core bus mirrors `$E00000-$FFFFFF`.
    index: usize,
    /// The value at the last poll. Seeded at arm time from the machine's *actual* RAM, which is what makes
    /// the very first poll a real comparison rather than a synthetic "first read" — see [`SymbolWatch::arm`].
    last: u8,
}

/// Every armed watch, polled once per loop iteration.
#[derive(Debug, Default)]
pub struct SymbolWatch {
    armed: Vec<Armed>,
}

impl SymbolWatch {
    /// Resolve every spec against the loaded listing and seed each baseline from `ram`.
    ///
    /// Returns the watch plus **one complaint per spec that could not be armed**. Those are errors and the
    /// caller must show them: a watch that silently watches nothing is indistinguishable from one that is
    /// working and has simply not fired yet, which is the single worst thing this feature could be.
    ///
    /// # The first-poll ruling
    ///
    /// The baseline is taken **here**, from RAM as it stands the moment the watch is armed — power-on
    /// zeros at startup, or whatever a reloaded ROM left behind. So there is no "first read" that fires
    /// with nothing to compare against, *and* a genuine change during the very first emulated frame is
    /// still a change against a real prior value and still fires. Both halves of the brief hold at once,
    /// which "skip the first poll" cannot manage. Pinned by `a_change_on_the_very_first_poll_still_fires`
    /// and `an_unchanged_value_is_silent_on_the_first_poll`.
    pub fn arm(
        specs: &[WatchSpec],
        symbols: Option<&SymbolTable>,
        ram: &[u8],
    ) -> (Self, Vec<String>) {
        let mut armed = Vec::new();
        let mut problems = Vec::new();
        for spec in specs {
            let name = &spec.symbol;
            let Some(symbols) = symbols else {
                problems.push(format!(
                    "symbol watch: cannot watch `{name}` — no symbol listing is loaded \
                     (build with `sigil build --emit-lst`, or check the earlier `symbols:` line)"
                ));
                continue;
            };
            let Some(addr) = symbols.address_of(name) else {
                problems.push(format!(
                    "symbol watch: cannot watch `{name}` — this ROM's listing has no such symbol \
                     (nothing will be reported for it)"
                ));
                continue;
            };
            if AddrSpace::of(addr) != AddrSpace::Ram {
                problems.push(format!(
                    "symbol watch: cannot watch `{name}` — it is at ${addr:06X}, which is not work RAM, \
                     so its value cannot change while the game runs"
                ));
                continue;
            }
            // The same mirror the core bus applies to `$E00000-$FFFFFF` (`bus.rs`: `& (RAM_SIZE - 1)`),
            // so this reads the byte the 68000 would read at that address.
            let index = (addr as usize) & (RAM_SIZE - 1);
            let Some(&last) = ram.get(index) else {
                problems.push(format!(
                    "symbol watch: cannot watch `{name}` — ${addr:06X} lands at RAM index {index}, past \
                     the {} bytes this machine has",
                    ram.len()
                ));
                continue;
            };
            armed.push(Armed {
                spec: spec.clone(),
                index,
                last,
            });
        }
        (SymbolWatch { armed }, problems)
    }

    /// Sample every armed byte and describe the ones that moved. Reading only; see the module doc.
    pub fn poll(&mut self, ram: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        for a in &mut self.armed {
            // Unreachable: `arm` refused any index this slice cannot hold, and work RAM does not resize.
            // Written as a skip rather than an index so a future caller with a short slice cannot panic
            // the render loop.
            let Some(&v) = ram.get(a.index) else { continue };
            if v != a.last {
                a.last = v;
                out.push(describe(&a.spec, v));
            }
        }
        out
    }
}

/// The line the owner reads. Four shapes, because there are four honestly different things to say.
///
/// The labelled case is the format the brief specified and the owner asked for, verbatim: `8/20 — Haze`.
/// The other three name the symbol, because without a label there is nothing else in the line to say
/// *which* watch spoke. None of them invents a name for a value that has none — the house rule is refuse
/// rather than guess, but never go quiet.
fn describe(spec: &WatchSpec, v: u8) -> String {
    let n = spec.labels.len();
    if n == 0 {
        return format!("{} = {v}", spec.symbol);
    }
    let Some(label) = spec.labels.get(v as usize) else {
        // Outside the list. `{v+1}/{n}` would read `23/20`, which is a lie about the list's length, so the
        // raw value is named instead and the shortfall is stated.
        return format!("{} = {v} — outside the {n} labels configured", spec.symbol);
    };
    if label.is_empty() {
        format!("{}/{n} — <no label> ({})", v as usize + 1, spec.symbol)
    } else {
        format!("{}/{n} — {label}", v as usize + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact case the parcel exists for, spelled the way aeon will paste it.
    fn scenes() -> WatchSpec {
        parse_spec(
            "Debug_Scene_Index: OJZ Default, OJZ Underwater, OJZ Windy, Shimmer Slow, Shimmer, \
             Shimmer Fast, Haze Slow, Haze, Haze Fast, Haze Uniform, Rocking Slow, Rocking, \
             Rocking Fast, Perspective Subtle, Perspective, Perspective Dramatic, Windy Haze, \
             Sky Haze, OJZ Caves, OJZ Locked Clouds",
        )
        .expect("the worked example must parse")
        .expect("and it is not blank")
    }

    /// A listing that puts `name` at `addr`, in the `.lst` shape `SymbolTable::parse` accepts.
    fn listing(name: &str, addr: u32) -> SymbolTable {
        SymbolTable::parse(&format!(
            "  Symbol Table (* = unused):\n\n {name} : {addr:X} C |\n\n   1 symbols\n"
        ))
        .expect("fixture listing must parse")
    }

    fn ram_with(index: usize, v: u8) -> Vec<u8> {
        let mut ram = vec![0u8; RAM_SIZE];
        ram[index] = v;
        ram
    }

    // ---- the grammar ----

    #[test]
    fn the_worked_example_parses_into_twenty_labels() {
        let s = scenes();
        assert_eq!(s.symbol, "Debug_Scene_Index");
        assert_eq!(s.labels.len(), 20, "the scene table has twenty rows");
        assert_eq!(s.labels[7], "Haze");
        assert_eq!(s.labels[19], "OJZ Locked Clouds");
    }

    #[test]
    fn a_blank_value_is_no_watch_and_not_an_error() {
        assert_eq!(parse_spec("").expect("blank is fine"), None);
        assert_eq!(parse_spec("   ").expect("blank is fine"), None);
    }

    #[test]
    fn a_value_with_no_symbol_is_a_named_error() {
        let e = parse_spec(": a, b").expect_err("a colon with nothing before it names no symbol");
        assert!(
            e.contains("Symbol: label"),
            "the error states the shape: {e}"
        );
    }

    /// A bare name is a legitimate watch: report the number, promise no name.
    #[test]
    fn a_bare_symbol_is_a_watch_with_no_labels() {
        let s = parse_spec("Level_Id").expect("valid").expect("not blank");
        assert_eq!(s.symbol, "Level_Id");
        assert!(s.labels.is_empty());
        // …and `Name:` is the same thing, not a one-entry list of the empty string.
        let colon = parse_spec("Level_Id:").expect("valid").expect("not blank");
        assert_eq!(colon, s, "a trailing colon adds no labels");
    }

    /// The file is hand-edited. A save must not reshuffle it, drop a gap, or lose a bare name.
    #[test]
    fn every_shape_round_trips_through_the_file_form() {
        for text in [
            "Debug_Scene_Index: OJZ Default, Haze, OJZ Caves",
            "Level_Id",
            "Gap: first, , third",
        ] {
            let spec = parse_spec(text).expect("valid").expect("not blank");
            let out = format_spec(&spec);
            assert_eq!(out, text, "not a fixed point");
            assert_eq!(
                parse_spec(&out).expect("valid").expect("not blank"),
                spec,
                "a second cycle moved"
            );
        }
        // The gap is positional, not a shorter list: index 2 is still `third`.
        let gap = parse_spec("Gap: first, , third").unwrap().unwrap();
        assert_eq!(gap.labels, vec!["first", "", "third"]);
    }

    // ---- arming, and being loud about failing to ----

    #[test]
    fn a_resolvable_ram_symbol_arms_and_says_nothing() {
        let (w, problems) = SymbolWatch::arm(
            &[scenes()],
            Some(&listing("Debug_Scene_Index", 0xFFFF_E50D)),
            &vec![0u8; RAM_SIZE],
        );
        assert!(problems.is_empty(), "clean arm complained: {problems:?}");
        assert_eq!(w.armed.len(), 1);
        // The 32-bit listing spelling is masked to the bus address and then mirrored into work RAM,
        // exactly as `bus.rs` does for `$E00000-$FFFFFF`.
        assert_eq!(w.armed[0].index, 0xE50D);
    }

    /// The whole point of the loudness rule: a config that names a symbol the listing does not have must
    /// SAY SO, once, visibly — not watch nothing forever while looking healthy.
    #[test]
    fn a_symbol_the_listing_lacks_is_a_loud_complaint() {
        let (w, problems) = SymbolWatch::arm(
            &[scenes()],
            Some(&listing("Something_Else", 0xFFFF_E50D)),
            &vec![0u8; RAM_SIZE],
        );
        assert!(w.armed.is_empty(), "an unresolvable watch must not arm");
        assert_eq!(
            problems.len(),
            1,
            "exactly one complaint, not zero: {problems:?}"
        );
        assert!(
            problems[0].contains("Debug_Scene_Index"),
            "the complaint names the symbol: {}",
            problems[0]
        );
        assert!(
            problems[0].contains("no such symbol"),
            "and says what is wrong: {}",
            problems[0]
        );
    }

    /// The commonest way to get nothing: a ROM built without `--emit-lst`, or a listing this player
    /// refused because it describes a different build. Either way the watch must not be silent.
    #[test]
    fn no_listing_at_all_is_a_loud_complaint() {
        let (w, problems) = SymbolWatch::arm(&[scenes()], None, &vec![0u8; RAM_SIZE]);
        assert!(w.armed.is_empty());
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("Debug_Scene_Index"));
        assert!(
            problems[0].contains("no symbol listing"),
            "names the real cause: {}",
            problems[0]
        );
    }

    /// A ROM symbol never changes, so watching one would look identical to a broken watch forever.
    #[test]
    fn a_non_ram_symbol_is_refused_out_loud() {
        let (w, problems) = SymbolWatch::arm(
            &[parse_spec("EntryPoint: a").unwrap().unwrap()],
            Some(&listing("EntryPoint", 0x200)),
            &vec![0u8; RAM_SIZE],
        );
        assert!(w.armed.is_empty());
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("not work RAM"), "{}", problems[0]);
        assert!(
            problems[0].contains("$000200"),
            "names the address: {}",
            problems[0]
        );
    }

    /// One bad entry must not take the good ones with it.
    #[test]
    fn a_bad_entry_does_not_disarm_the_good_ones() {
        let table = listing("Debug_Scene_Index", 0xFFFF_E50D);
        let (w, problems) = SymbolWatch::arm(
            &[parse_spec("Not_A_Symbol: x").unwrap().unwrap(), scenes()],
            Some(&table),
            &vec![0u8; RAM_SIZE],
        );
        assert_eq!(problems.len(), 1, "one complaint for the one bad entry");
        assert_eq!(w.armed.len(), 1, "the good entry still armed");
    }

    // ---- the first-poll ruling ----

    /// No synthetic "first read" toast: the baseline came from real RAM at arm time, so a value that has
    /// not moved says nothing however early the first poll is.
    #[test]
    fn an_unchanged_value_is_silent_on_the_first_poll() {
        let ram = ram_with(0xE50D, 7);
        let (mut w, problems) = SymbolWatch::arm(
            &[scenes()],
            Some(&listing("Debug_Scene_Index", 0xFFFF_E50D)),
            &ram,
        );
        assert!(problems.is_empty());
        assert!(
            w.poll(&ram).is_empty(),
            "the very first poll invented a change out of the seed value"
        );
        assert!(w.poll(&ram).is_empty(), "and stays quiet");
    }

    /// The other half of the ruling, which "skip the first poll" would break: a game that writes the
    /// value during frame 1 has genuinely changed it, and the owner must be told.
    #[test]
    fn a_change_on_the_very_first_poll_still_fires() {
        let table = listing("Debug_Scene_Index", 0xFFFF_E50D);
        let boot = vec![0u8; RAM_SIZE]; // power-on: the index is 0
        let (mut w, problems) = SymbolWatch::arm(&[scenes()], Some(&table), &boot);
        assert!(problems.is_empty());
        // Frame 1 sets it to 3.
        let after = ram_with(0xE50D, 3);
        assert_eq!(
            w.poll(&after),
            vec!["4/20 — Shimmer Slow".to_string()],
            "a genuine frame-1 change was swallowed"
        );
    }

    // ---- reporting ----

    #[test]
    fn a_labelled_change_reads_exactly_as_asked() {
        let table = listing("Debug_Scene_Index", 0xFFFF_E50D);
        let (mut w, _) = SymbolWatch::arm(&[scenes()], Some(&table), &vec![0u8; RAM_SIZE]);
        // The owner's own example: index 7 is `Haze`, and he wants to read `8/20`.
        assert_eq!(
            w.poll(&ram_with(0xE50D, 7)),
            vec!["8/20 — Haze".to_string()]
        );
        // Only on the edge — a value that stays put says nothing on the frames after.
        assert!(w.poll(&ram_with(0xE50D, 7)).is_empty());
        assert_eq!(
            w.poll(&ram_with(0xE50D, 19)),
            vec!["20/20 — OJZ Locked Clouds".to_string()]
        );
    }

    /// "Refuse rather than guess, but never go quiet." A value with no label still reports its number and
    /// says outright that the name is missing — it never invents one, and it never falls silent.
    #[test]
    fn a_value_with_no_label_still_reports() {
        let table = listing("Gap", 0xFFFF_E50D);
        let spec = parse_spec("Gap: a, , c").unwrap().unwrap();
        let (mut w, _) = SymbolWatch::arm(&[spec], Some(&table), &vec![0u8; RAM_SIZE]);
        // Index 1's label is blank.
        let blank = w.poll(&ram_with(0xE50D, 1));
        assert_eq!(blank, vec!["2/3 — <no label> (Gap)".to_string()]);
        // Index 5 is off the end of a three-entry list. `6/3` would be a lie about the list, so the raw
        // value is named and the shortfall stated.
        let past = w.poll(&ram_with(0xE50D, 5));
        assert_eq!(
            past,
            vec!["Gap = 5 — outside the 3 labels configured".to_string()]
        );
        // Neither case is silent, which is the property that matters.
        assert!(!blank.is_empty() && !past.is_empty());
    }

    /// A watch with no labels at all is a number readout, and says so plainly rather than pretending to a
    /// list of length zero (`1/0` would be nonsense).
    #[test]
    fn a_watch_with_no_labels_reports_the_number() {
        let table = listing("Level_Id", 0xFFFF_E50D);
        let spec = parse_spec("Level_Id").unwrap().unwrap();
        let (mut w, _) = SymbolWatch::arm(&[spec], Some(&table), &vec![0u8; RAM_SIZE]);
        assert_eq!(
            w.poll(&ram_with(0xE50D, 12)),
            vec!["Level_Id = 12".to_string()]
        );
    }

    /// Two watches on two different bytes are independent, and one moving does not report the other.
    #[test]
    fn two_watches_report_independently() {
        let table = SymbolTable::parse(
            "  Symbol Table (* = unused):\n\n A : FFFFE50D C |\n B : FFFFE600 C |\n\n   2 symbols\n",
        )
        .expect("fixture parses");
        let specs = [
            parse_spec("A: zero, one").unwrap().unwrap(),
            parse_spec("B: red, green").unwrap().unwrap(),
        ];
        let (mut w, problems) = SymbolWatch::arm(&specs, Some(&table), &vec![0u8; RAM_SIZE]);
        assert!(problems.is_empty(), "{problems:?}");
        let mut ram = vec![0u8; RAM_SIZE];
        ram[0xE600] = 1;
        assert_eq!(
            w.poll(&ram),
            vec!["2/2 — green".to_string()],
            "A must stay quiet"
        );
        ram[0xE50D] = 1;
        assert_eq!(w.poll(&ram), vec!["2/2 — one".to_string()]);
    }

    /// A word read at this symbol would splice in the neighbouring byte. The brief said "word"; the ROM
    /// says `u8` at an odd address. This pins the byte, so a later "make it a word" edit fails here rather
    /// than reporting numbers the game never held.
    #[test]
    fn the_read_is_one_byte_and_ignores_its_neighbours() {
        let table = listing("Debug_Scene_Index", 0xFFFF_E50D);
        let mut ram = vec![0u8; RAM_SIZE];
        ram[0xE50D] = 7;
        let (mut w, _) = SymbolWatch::arm(&[scenes()], Some(&table), &ram);
        // Both neighbours change; the watched byte does not.
        ram[0xE50C] = 0xFF;
        ram[0xE50E] = 0xFF;
        assert!(
            w.poll(&ram).is_empty(),
            "a neighbouring byte moved the reported value — this is not a byte read"
        );
    }
}
