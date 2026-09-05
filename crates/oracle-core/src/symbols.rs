//! Symbol table — turn raw hex addresses back into the names the programmer wrote
//! (`docs/2026-08-14-tooling-frontier-recon.md` §1b, §6 item 2).
//!
//! Every address oracle-next reports today is raw hex. This module is the other half: parse the
//! `<rom>.lst` listing Aeon's `sigil` build emits, and answer the three questions a debugger actually
//! asks — *what address is `Player_1`?*, *what is the PC `$00021A` inside of?*, and *what symbols start
//! with `Ring_`?*
//!
//! # No I/O, by charter
//!
//! `oracle-core` is the "deterministic, no-I/O" core (see the crate doc), so [`SymbolTable::parse`] takes
//! a `&str` and the **caller** reads the file. A symbol table is also never part of `System`, the state
//! hash, or `export_state`: it is caller-owned metadata *about* a ROM, exactly like [`crate::watchpoints`]
//! is caller-owned observation of one. It therefore cannot move a currency hash.
//!
//! # The input format (verified against the emitter, not guessed)
//!
//! The producer is `sigil/crates/sigil-link/src/listing.rs::emit_listing`. It writes **two** views of the
//! same address-sorted symbol list, because two different consumers each read one half, and then a third
//! section that is not a symbol list at all:
//!
//! ```text
//! (0) 1971/FFFF8CFA :        Player_1:            <- body line   ("(depth) idx/HEXADDR :        Name:")
//!  ...
//!   Symbol Table (* = unused):                    <- section header
//!   --------------------------
//!
//!  Player_1 : FFFF8CFA C |                        <- table row   ("[*]Name : HEXADDR <C|-> |")
//!  ...
//!    2129 symbols                                 <- footer
//!     0 unused symbols
//!
//!   Equate Table (name = value; values, not addresses):   <- section header (sigil 0df77f83, 2026-08-19)
//!   ---------------------------------------------------
//!
//! EQU AF_BACK = $000000FE                                 <- equate row ("EQU <name> = $<hex>")
//!  ...
//!    682 equates                                          <- footer
//! ```
//!
//! We prefer the **table rows**: they are the richer half, carrying the `*` unused marker and the
//! `C`(code)/`-`(equate) type marker that the body lines drop. Body lines are parsed only as a fallback,
//! for a listing that has no `Symbol Table` section at all. See [`TableSource`].
//!
//! # The `Equate Table` is ingested into a namespace of its OWN — never into the symbol table
//!
//! Sigil began emitting the third section on 2026-08-19. Its rows are **values, not addresses** — sigil's
//! own debugger-map test enforces that they never resolve as code or RAM locations — so folding them into
//! the symbol table would change what [`SymbolTable::address_of`] means and let a constant answer an
//! addr→name query. `F-EQUATES-NAMESPACE` (`docs/2026-08-19-cram-serve-recon.md` §6.3) registered that as
//! a decision to be **ruled**, and until 2026-09-05 this module consumed the section — header, rule, rows
//! and `N equates` trailer — without counting it as damage, storing nothing but the two counts.
//!
//! **It is ruled**: oracle CR-M, adopted as `contract/protocol.md` §11.36 (empyrean `2208aa8`), **option
//! A**. Equates are readable, through a **second map that is not the symbol table** —
//! [`SymbolTable::equate_value`] and [`SymbolTable::equates_with_prefix`] — and they are reachable by
//! **name only**. The measurement that decided the shape, re-derived here on
//! `fixtures/aeon/s4.debug.lst`: 724 equates, 2,743 labels, **zero name collisions**, and **661 of the 724
//! equate values fall inside the cart window** `$000000–$3FFFFF`. Sixty-seven of them are numerically
//! equal to the address of a real label. Folding equates into the symbol vector would make
//! `ANI_DUST_PUFF = $2AF9C` indistinguishable from the label *at* `$2AF9C` on every `addr→name` path —
//! including paths not yet written — so the separation is **structural**: an equate is never pushed into
//! `syms`, so it can never enter `rev`, `by_name`, `by_demangled` or `by_module`, and no reverse lookup
//! has to *remember* to filter it out. The test
//! `equates_are_not_addressable_in_either_direction` asserts that against the real listing, on an
//! equate whose value is a real label's address.
//!
//! Before the recognition was deliberate the rows merely happened to be rejected by the five-token row
//! shape, which cost 684 phantom `skipped_lines` on the real `s4.lst` and made [`SymbolTable::is_intact`]
//! wrong about a healthy file.
//!
//! # The other dialect: stock AS listings (`sonic.lst`, the classic disassemblies)
//!
//! Sigil's emitter is one producer of this format; the **AS macro assembler itself** is the other, and the
//! classic Sonic disassemblies (`s1disasm`, `s2disasm`, `skdisasm`) are what Aurora's classic loop loads.
//! AS writes the same `Symbol Table` section with three differences, each of which silently drops symbols
//! if it is not handled. All three were measured against the real `s1disasm/sonic.lst` (12,410 symbols):
//!
//! 1. **Two symbols per line, `|`-separated.** AS packs the table two columns wide —
//!    `Sym : ADDR C |  Sym2 : ADDR C |` — so a row-per-line reader sees a ten-token line and rejects it.
//!    Measured: 4,174 of `sonic.lst`'s rows are the two-symbol shape. The fix is to **split on `|` before
//!    tokenising**, which subsumes sigil's one-per-line form (its trailing `|` yields one entry and one
//!    empty tail) rather than branching on a per-file dialect flag. There is no per-file signal worth
//!    keying on: the shape is decidable per row, and a producer that ever mixed the two would still parse.
//! 2. **RAM addresses are 48-bit sign-extended** — `f_debugmode : FFFFFFFFFFFFFFFA C`, sixteen hex digits,
//!    which overflows `u32` and is skipped. Measured: 932 such rows in `sonic.lst`, **and every address the
//!    classic loop needs is one of them** (all of RAM). Values are therefore parsed as `u64` and masked
//!    with [`BUS_ADDR_MASK`], which lands `FFFFFFFFFFFFD000` on the bus address `$FFD000` exactly as it
//!    lands sigil's `FFFF8CFA` on `$FF8CFA` — the same trap 2 rule, one width wider.
//! 3. **Not every row carries an address.** AS emits its own build metadata as pseudo-symbols whose value
//!    is a quoted string or a float (`ARCHITECTURE : "x86_64-unknown-linux" -`, `CONSTPI :
//!    3.141592653589793 -`, and `TIME : "11:35:59 PM" -`, whose value even contains a space). These are
//!    **recognised and consumed, never ingested** — counted in [`SymbolTable::non_address_rows`] rather
//!    than in [`SymbolTable::skipped_lines`], for exactly the reason the `Equate Table` is: five rows of
//!    normal, healthy output must not make [`SymbolTable::is_intact`] report a whole file as damaged.
//!    Measured on `sonic.lst`: 12,405 addresses + 5 non-address rows == the footer's 12,410, and
//!    1,568 + 5 starred == the footer's 1,573 unused, so both of the file's own checksums close.
//!
//! ## The type column, per dialect — and why the two are handled differently
//!
//! An AS `-` row is an **equate**, and in AS land a RAM address is legitimately equ-defined: `sonic.lst`
//! spells `v_player : FFFFFFFFFFFFD000 -`. But the same `-` also covers genuine constants — 2,158 of
//! `sonic.lst`'s 2,231 `-` rows are not RAM addresses at all, and 1,236 of them hold values below `$100`.
//! Ingesting them wholesale would put `AniArt_MZ_Lava.size = 8` into nearest-preceding reverse lookup;
//! skipping them wholesale would lose `v_player`. So the ruling is **forward-only**:
//!
//! - **name → value resolves** ([`by_name`](SymbolTable::by_name), [`address_of`](SymbolTable::address_of),
//!   the prefix searches). This is the primary use — poking RAM by symbol.
//! - **value → name does not** ([`resolve`](SymbolTable::resolve), [`symbols_at`](SymbolTable::symbols_at)).
//!   A constant answering an addr→name query is the confidently-wrong answer this module exists to avoid,
//!   and it is the exact failure shape the ROM-binding guard is built against.
//!
//! [`Symbol::resolves_in_reverse`] is that predicate, and it is a property of the **row**, not of the file.
//!
//! **This is NOT the `Equate Table` rule and must not be unified with it.** Sigil's equates arrive in their
//! own section, and sigil's own emitter pins them as values-never-addresses (its debugger-map test enforces
//! it), while sigil RAM labels arrive as ordinary type-`C` `Symbol Table` rows. So for the sigil dialect
//! there is nothing an equate could legitimately answer, and the never-resolvable rule below stays exactly
//! as shipped. Two dialects, two contracts; the `-` type column and the `Equate Table` section are
//! different things that happen to share a word.
//!
//! ⚑ **CR-M did not touch this half, deliberately.** §11.36's scope is *the `Equate Table` section*: a
//! type-`-` row is still an ordinary [`Symbol`], still forward-resolvable, and is **not** reachable through
//! [`SymbolTable::equate_value`]. Unifying them would be wrong in both directions — an AS `-` row can
//! legitimately *be* a RAM address (`v_player`), so publishing it as an equate value would put an address
//! in a field the contract says is not one; and a sigil equate is never an address, so putting it in the
//! symbol table is the exact fold option A was chosen to prevent. The two populations are disjoint by
//! construction: only `Section::EquateTable` rows reach the equate map, and only `Section::SymbolTable`
//! rows reach `syms`.
//!
//! ## Four traps, each of which silently produces wrong answers
//!
//! 1. **Sigil's RAM addresses are plain 8-hex `FFFFxxxx`** — *not* the 48-bit sign-extended
//!    `FFFFFFFFFFFFxxxx` AS emits (both are accepted; see the dialect section above). (Aeon's own
//!    `tools/s4budget.py::_is_ram_addr_str` still tests only for the sign-extended form and consequently
//!    reports `RAM: 0 bytes` on a sigil listing — a live bug in their tree, deliberately not copied.)
//! 2. **`FFFFxxxx` is a 32-bit spelling of a 24-bit bus address.** The 68000 drives 24 address lines, and
//!    this core decodes work RAM at `$E00000–$FFFFFF` (`bus.rs`), so the listing's `FFFF8CFA` *is* the
//!    machine's `$FF8CFA`. Matching a PC or a bus address against the raw value finds nothing, every time.
//!    Each [`Symbol`] therefore carries both [`raw_addr`](Symbol::raw_addr) (as written in the file) and
//!    [`addr`](Symbol::addr) (masked to 24 bits); **all lookups use the 24-bit form**, and mask the query
//!    too, so a caller may pass either spelling.
//! 3. **The type column does not discriminate code from RAM.** Aeon's equates arrive in a section of their
//!    own (below) rather than in the type column, so every `Symbol Table` row of the real `s4.lst` is `C` —
//!    including every RAM variable. AS uses `-` for equates, but an AS equate is just as likely to *be* a
//!    RAM address (`v_player`). Either way code and RAM are separable only by *address range*, which is
//!    what [`AddrSpace`] does — never by the type column.
//! 4. **Symbols bind per-game AND per-build-shape.** `if DEBUG == 1 @shape_divergent` blocks in the
//!    engine's `ram.emp` shift the RAM layout between shapes: of the symbols `s4.lst` and `s4.debug.lst`
//!    share, **92.6% resolve to a different address**, with divergence starting at `BootData` (`$3A0` vs
//!    `$3B0`) and never recovering. A mismatched table is not degraded — it is confidently wrong. See
//!    [`SymbolTable::validate_against_rom`].
//!
//! # The `$` scope tree
//!
//! Sigil mangles a proc-local label as `$<module.path>$<Parent>$<local>` (e.g.
//! `$engine.boot$EntryPoint$wait_dma`). Splitting on `$` yields a multi-level scope at zero build cost, so
//! a PC in the middle of a long routine resolves to `EntryPoint.wait_dma` rather than
//! "`EntryPoint` + $14 — somewhere". Both spellings are kept: [`Symbol::name`] is the raw mangled name
//! (what the file said), [`Symbol::demangled`] is the readable form, and [`Symbol::scope`] exposes the
//! levels. The demangling reproduces sigil's own `demangle_symbols`, so a name printed here reads the same
//! as one shown by the on-target MD Debugger.
//!
//! **The module is found by the dot, not by position** — a label emitted inside a macro puts the macro
//! instance *outside* the module (`$diag2$engine.bg_anim$raise`), so the positional reading invents phantom
//! modules. See [`scope_parts`] for the evidence and the rule.
//!
//! Sigil *drops* its plumbing symbols (`__align$…`, `$…$asm<N>$…`) when it builds the on-ROM appendix. We
//! keep them — they are real addresses and make nearest-preceding resolution *tighter* — but flag them via
//! [`Symbol::is_synthetic`] so a caller can prefer a source-meaningful name.

use std::collections::BTreeMap;
use std::fmt;

/// The 68000 drives 24 address lines; every address above that is an alias. Listing values such as
/// `FFFF8CFA` are masked with this to get the address the machine actually puts on the bus (`$FF8CFA`).
pub const BUS_ADDR_MASK: u32 = 0x00FF_FFFF;

/// What kind of value a row declares, from the listing's type column. Note trap 3: this does **not**
/// separate code from RAM — Aeon emits `Code` for everything, variables included.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymbolKind {
    /// `C` — an address in the assembled image's address space (the only kind Aeon currently emits).
    Code,
    /// `-` — an equate, as spelled in the `Symbol Table`'s **type column**. Never seen in a real Aeon
    /// listing (sigil's equates arrive in the separate `Equate Table` section instead, which this module
    /// recognises but does not ingest); ubiquitous in stock AS listings, where 2,231 of `sonic.lst`'s
    /// 12,410 rows carry it and a handful of them — `v_player` among them — are genuine RAM addresses.
    ///
    /// These rows are ingested **forward-only**: see [`Symbol::resolves_in_reverse`] and the module docs.
    Equate,
}

/// Which half of the listing a table was built from. Recorded so a caller can tell a full listing from a
/// body-only one (the fallback path loses the `unused` and `Equate` markers, which body lines do not carry).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableSource {
    /// The `Symbol Table (* = unused):` section — the preferred, richer half.
    SymbolTable,
    /// The `(depth) idx/HEXADDR :        Name:` body lines — used only when no section header was found.
    BodyLines,
}

/// Which region of the 68000 bus map an address falls in. This is the *only* way to tell a code label from
/// a RAM variable in this format (trap 3), and it is what stops a RAM query from resolving to the last ROM
/// symbol with a nonsense displacement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddrSpace {
    /// `$000000–$3FFFFF` — cartridge ROM.
    Rom,
    /// `$E00000–$FFFFFF` — 64 KiB work RAM (mirrored); where every `FFFFxxxx` listing value lands.
    Ram,
    /// Anything else — I/O, VDP ports, the open-bus gaps. No Aeon symbol lives here.
    Other,
}

impl AddrSpace {
    /// Classify a **24-bit** bus address. Callers passing a 32-bit spelling should mask first (or use the
    /// [`SymbolTable`] lookups, which mask for them).
    pub fn of(addr: u32) -> Self {
        match addr & BUS_ADDR_MASK {
            0x00_0000..=0x3F_FFFF => AddrSpace::Rom,
            0xE0_0000..=0xFF_FFFF => AddrSpace::Ram,
            _ => AddrSpace::Other,
        }
    }
}

/// The levels a mangled name decomposes into. A plain (unmangled) name has only
/// [`local`](Scope::local).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Scope<'a> {
    /// `__offsets` / `__align` / `diag2` — a synthetic scope wrapped *around* the module, when one is
    /// present. See [`scope_parts`] for why this level exists.
    pub outer: Option<&'a str>,
    /// `engine.boot` — the `.emp` module path, if the name was mangled.
    pub module: Option<&'a str>,
    /// `EntryPoint` — the enclosing proc, if the name has a level between the module and the label.
    pub parent: Option<&'a str>,
    /// `wait_dma` — the innermost label. Always present; equals the whole name for plain symbols.
    pub local: &'a str,
}

/// One symbol: a name (in both spellings), an address (in both widths), and the listing's markers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Symbol {
    /// The name exactly as the listing spells it, mangling included (`$engine.boot$EntryPoint$wait_dma`).
    pub name: String,
    /// The readable form (`EntryPoint.wait_dma`); identical to [`name`](Symbol::name) for plain symbols.
    /// Not guaranteed unique — two modules may each hold a `Parent.local` pair with the same spelling.
    pub demangled: String,
    /// The address as written in the file, unmasked (`0xFFFF_8CFA`). Kept so a caller can round-trip a
    /// value back to the listing's own spelling.
    ///
    /// **Low 32 bits.** AS's 48-bit sign-extended spelling (`FFFFFFFFFFFFD000`) is parsed as `u64` and
    /// truncated here, which yields exactly the conventional 32-bit form (`0xFFFF_D000`) — the same value
    /// sigil would have written for the same location. The invariant `addr == raw_addr & BUS_ADDR_MASK`
    /// therefore holds at both widths.
    pub raw_addr: u32,
    /// The 24-bit bus address (`0xFF_8CFA`) — **what every lookup here matches against** (trap 2).
    pub addr: u32,
    /// `C` or `-`. Does not discriminate code from RAM (trap 3) — use [`AddrSpace::of`] for that. It
    /// **does** decide reverse-lookup eligibility: see [`resolves_in_reverse`](Symbol::resolves_in_reverse).
    pub kind: SymbolKind,
    /// The listing's `*` marker. Always `false` in practice: real Aeon listings end `0 unused symbols`.
    pub unused: bool,
    /// Compiler plumbing rather than a name anyone wrote — an `__align$…` pad or a synthetic `asm<N>`
    /// block scope. Sigil drops these from the on-ROM appendix; we keep them (a closer nearest-preceding
    /// answer) but mark them so a caller can prefer a source-meaningful name.
    pub is_synthetic: bool,
    /// Some other symbol in the same table shares this [`demangled`](Symbol::demangled) spelling at a
    /// **different** address, so the readable name does not identify a location on its own. Real and not
    /// rare: a macro expanded 46 times emits 46 labels that all demangle to `engine.bg_anim.raise`
    /// (24 such collisions in `s4.debug.lst`). [`Resolution`]'s `Display` falls back to the raw mangled
    /// name — which is always unique — whenever this is set.
    pub demangled_ambiguous: bool,
}

impl Symbol {
    /// The name split into its `$` levels. Borrows from [`name`](Symbol::name) — no allocation.
    pub fn scope(&self) -> Scope<'_> {
        scope_parts(&self.name)
    }

    /// Which region of the bus map this symbol lives in.
    pub fn space(&self) -> AddrSpace {
        AddrSpace::of(self.addr)
    }

    /// May this symbol answer an **addr → name** query ([`SymbolTable::resolve`],
    /// [`SymbolTable::symbols_at`])?
    ///
    /// True for [`SymbolKind::Code`], false for [`SymbolKind::Equate`] — the forward-only ruling in the
    /// module docs. An AS `-` row may be a RAM address (`v_player`) or a bare constant (`…​.size = 8`), and
    /// the row carries nothing that tells the two apart; forward resolution serves the address case with no
    /// downside, while admitting the constants into nearest-preceding search would invent confident names
    /// for low addresses. Name → value is unaffected in both directions of that trade.
    ///
    /// Every symbol from a sigil listing is `Code`, so this is `true` for all of them and the reverse
    /// direction is unchanged for the dialect this module was written against.
    pub fn resolves_in_reverse(&self) -> bool {
        self.kind == SymbolKind::Code
    }
}

/// A resolved address: the symbol it landed in (or on) and how far past that symbol's start it is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Resolution<'a> {
    /// The nearest symbol at or before the queried address, in the same [`AddrSpace`].
    pub symbol: &'a Symbol,
    /// Queried address minus the symbol's address; `0` for an exact hit.
    pub displacement: u32,
}

impl Resolution<'_> {
    /// The symbol's **identifying** spelling, with no displacement suffix — the one a caller may hand
    /// back to [`SymbolTable::address_of`] and get this symbol again.
    ///
    /// This is [`Display`](fmt::Display)'s name half without its `+$hex` tail, and the two are
    /// deliberately different things. Display is for a human reading a disassembly line; this is for any
    /// consumer that must round-trip the name, which on the Aether bus is every `symbol` field — the wire
    /// schema's `$defs/symbolName` rejects a `+$hex` suffix *by pattern*, and `protocol.md` §4 puts the
    /// displacement in its own number so it never has to be parsed back out of a string.
    ///
    /// Falls back to the raw mangled name when the readable one is
    /// [ambiguous](Symbol::demangled_ambiguous), for Display's reason: a name several addresses share
    /// does not identify one, and the raw name always does.
    pub fn name(&self) -> &str {
        if self.symbol.demangled_ambiguous {
            &self.symbol.name
        } else {
            &self.symbol.demangled
        }
    }
}

impl fmt::Display for Resolution<'_> {
    /// `EntryPoint.wait_dma` for an exact hit, `EntryPoint.wait_dma+$1A` otherwise — the form a
    /// disassembly listing or a watch-hit dump wants.
    ///
    /// Falls back to the raw mangled name when the readable one is
    /// [ambiguous](Symbol::demangled_ambiguous): a name that several different addresses share does not
    /// identify a location, and printing it anyway is precisely the confidently-wrong output this module
    /// exists to avoid. The raw name is unique, so the answer stays exact even when it is uglier.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The name half is `name()`, not a second copy of its ambiguity rule: the two spellings differ
        // only by the displacement suffix, and letting them derive the name independently is how they
        // would come to disagree about which name a colliding symbol gets.
        let name = self.name();
        if self.displacement == 0 {
            write!(f, "{name}")
        } else {
            write!(f, "{name}+${:X}", self.displacement)
        }
    }
}

/// Why a listing could not be parsed at all. Individual malformed *lines* are never an error — they are
/// skipped and counted in [`SymbolTable::skipped_lines`], because a listing that gains an unrecognised
/// decoration should still yield its symbols.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymbolParseError {
    /// Neither a `Symbol Table` section nor any body line produced a single symbol. Almost certainly not
    /// a listing file.
    NoSymbols,
}

impl fmt::Display for SymbolParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SymbolParseError::NoSymbols => {
                write!(f, "no symbols found (not a sigil/AS `.lst` listing?)")
            }
        }
    }
}

impl std::error::Error for SymbolParseError {}

/// The verdict on whether a parsed table belongs to a given ROM image. Three-state on purpose: "we cannot
/// tell" is a different answer from "it does not match", and the caller's policy for the two differs
/// (warn-and-use vs refuse).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RomBinding {
    /// The ROM carries a `deb2` symbol appendix exactly where this listing's `EndOfRom` says it should.
    Match {
        /// The `EndOfRom` value == the appendix's offset in the image.
        appendix_offset: u32,
        /// Bytes from the appendix start to end-of-image.
        appendix_len: usize,
    },
    /// Positively wrong — this listing does not describe this image.
    Mismatch(BindingFault),
    /// Nothing to key on. Not an error: a hand-written or non-Aeon listing simply cannot be checked.
    Indeterminate(Indeterminate),
}

/// The specific way a listing failed to bind to a ROM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindingFault {
    /// `EndOfRom` points past the end of the image (or leaves no room for the 2-byte magic).
    EndOfRomOutOfRange { end_of_rom: u32, rom_len: usize },
    /// The image has no `de b2` magic at `EndOfRom` — the load-bearing check. This is what catches
    /// `s4.debug.lst` against `s4.bin` (verified in both directions).
    NoAppendixMagic { offset: u32, found: [u8; 2] },
    /// The magic is there but the appendix is implausibly short (< [`DEB2_MIN_LEN`]).
    AppendixTooSmall { offset: u32, len: usize },
}

/// Why no verdict was possible.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Indeterminate {
    /// The listing declares no `EndOfRom` symbol, so there is no offset to probe.
    NoEndOfRomSymbol,
    /// `EndOfRom` is **exactly** the image length: the listing describes a ROM with no appendix after it.
    /// A symbol one past the last byte cannot carry the magic and cannot be a mismatched in-range offset,
    /// so there is nothing to probe and nothing to contradict. See
    /// [`SymbolTable::validate_against_rom`].
    EndOfRomIsImageEnd { rom_len: usize },
}

/// Magic at the head of the `deb2` symbol appendix every sigil-built Aeon ROM carries at `EndOfRom`.
/// Verified firsthand: `s4.bin` `0x000A11F0` = `de b2 04 02 …`. This is byte-for-byte the check the
/// on-target MD Debugger blob itself performs (`cmpi.w #$DEB2,(a1)+`) to find its own symbol table.
pub const DEB2_MAGIC: [u8; 2] = [0xDE, 0xB2];

/// Floor for a plausible appendix, copied from sigil's own `SONIC4_APPENDIX_FLOOR`. Observed real sizes
/// span 29,603–43,474 bytes, so this rejects a coincidental two-byte match near end-of-image without
/// coming close to a real table.
pub const DEB2_MIN_LEN: usize = 0x2000;

/// The symbol named `EndOfRom` — sigil places the appendix there, so it is the one value in the whole
/// listing derivable from the ROM bytes.
const END_OF_ROM_SYMBOL: &str = "EndOfRom";

/// Offset of the ROM header's "last address in image" longword (`$1A4`), re-fixed by sigil *after* the
/// appendix is appended. See [`rom_declared_end`].
pub const ROM_HEADER_END_OFF: usize = 0x1A4;

/// Read the ROM header's declared last-address longword (`$1A4`). For a complete sigil-built image this
/// equals `rom.len() - 1` — verified 5/5 across `s4`, `s4.debug`, `demo.debug`, `s4.stress`,
/// `s4.stressart`. Exposed as a **separate** helper rather than folded into
/// [`SymbolTable::validate_against_rom`] because it validates the ROM against *itself* (is this a whole,
/// untruncated image?) and says nothing about which listing describes it; a legitimately pad-to-power-of-two
/// ROM would also fail it.
pub fn rom_declared_end(rom: &[u8]) -> Option<u32> {
    let b = rom.get(ROM_HEADER_END_OFF..ROM_HEADER_END_OFF + 4)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// A parsed listing: symbols address-sorted, plus name indexes for both spellings and a module index.
///
/// Immutable once built. All lookups are `O(log n)`; construction is a single pass plus one sort.
#[derive(Clone, Debug)]
pub struct SymbolTable {
    /// Sorted by `(addr, name)`. The sort key is the **24-bit** address (trap 2).
    syms: Vec<Symbol>,
    /// Indexes into [`syms`](Self::syms), in the same `(addr, name)` order, restricted to the symbols that
    /// may answer an addr→name query ([`Symbol::resolves_in_reverse`]). **Every** reverse lookup binary
    /// searches this and never `syms`, which is what makes the forward-only ruling structural rather than
    /// a filter each call site has to remember. Identical to `0..syms.len()` for any sigil listing.
    rev: Vec<usize>,
    /// Raw mangled name → index. `BTreeMap`, not `HashMap`: ordered iteration makes prefix search a range
    /// query, and the crate bans `HashMap` in anything that might be hashed or serialized.
    by_name: BTreeMap<String, usize>,
    /// Demangled name → indexes. A `Vec` because `Parent.local` is not unique across modules.
    by_demangled: BTreeMap<String, Vec<usize>>,
    /// `.emp` module path → indexes, for the scope tree.
    by_module: BTreeMap<String, Vec<usize>>,
    source: TableSource,
    declared_count: Option<usize>,
    declared_unused: Option<usize>,
    skipped_lines: usize,
    /// `None` when the listing carries no `Equate Table` section at all — every listing sigil emitted
    /// before 2026-08-19, and every AS listing.
    equates: Option<EquateSection>,
    non_address: NonAddressRows,
}

/// `Symbol Table` rows that are well-formed but carry no address — AS's own build metadata
/// (`ARCHITECTURE`, `CONSTPI`, `DATE`, `MOMCPUNAME`, `TIME`), whose "value" is a quoted string or a float.
///
/// Recognised and consumed rather than skipped, and counted here rather than in `skipped_lines`, so that
/// the file's own two checksums still close: the `N symbols` footer counts these rows, and so does
/// `N unused symbols` (all five are `*`-marked on `sonic.lst`). Without the accounting a perfectly healthy
/// AS listing reports as damaged, [`SymbolTable::is_intact`] goes false, and the frontend's load policy
/// **refuses** it — the same failure the `Equate Table` recognition was added to prevent, one dialect over.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct NonAddressRows {
    /// How many such rows were seen.
    rows: usize,
    /// How many of them carried the `*` unused marker.
    unused: usize,
}

/// What we keep of the `Equate Table`: the two counts **and the values**, in a map of their own.
///
/// ⚑ **`by_name` is a separate map rather than a flag on a shared one, and that is the whole ruling.**
/// §11.36 chose option A because 661 of this listing's 724 equate values fall inside the cart window: a
/// `kind` flag on a shared table would make every present and future `addr→name` path correct only for as
/// long as each of them remembered to test it. There is nothing to remember here — an equate is not a
/// [`Symbol`], is never pushed into `syms`, and therefore cannot reach `rev`, which is the one index every
/// reverse lookup binary-searches.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct EquateSection {
    /// Rows matching `EQU <name> = $<hex>` that we recognised and consumed.
    rows: usize,
    /// The section's own `N equates` trailer, when it is there at all.
    declared: Option<usize>,
    /// Name → value, ordered. `BTreeMap` for the same two reasons `by_name` is one: a prefix search is a
    /// range query, and the crate bans `HashMap` in anything that might be hashed or serialized.
    ///
    /// A duplicate equate name cannot occur in a real listing (measured: 724 rows, 724 distinct names on
    /// `fixtures/aeon/s4.debug.lst`); if one ever does, the last row wins, deterministically, and `rows`
    /// still counts the rows so the trailer check still closes.
    by_name: BTreeMap<String, u64>,
}

/// Everything [`SymbolTable::parse`] learns that is not a [`Symbol`]. Bundled so [`SymbolTable::build`]
/// keeps a readable signature as the format grows sections.
struct ParseCounts {
    declared_count: Option<usize>,
    declared_unused: Option<usize>,
    skipped_lines: usize,
    equates: Option<EquateSection>,
    non_address: NonAddressRows,
}

/// Which of the listing's three regions the line cursor is in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Section {
    /// Before any section header — the `(depth) idx/HEXADDR : Name:` body lines.
    Body,
    /// Inside `Symbol Table (* = unused):`.
    SymbolTable,
    /// Past the symbol table's own `N symbols` footer. Nothing here is a symbol row — a stock AS listing
    /// continues with page headers, a `Defined Macros:` list (`|`-packed, exactly like the symbol table)
    /// and an assembly-statistics block, 734 lines of it on `sonic.lst`. Reading rows to end-of-file
    /// counted all of it as damage; the footer is where the section ends, so that is where row parsing
    /// stops. The `N unused symbols` footer is still collected here, because AS emits it after the count.
    AfterSymbolTable,
    /// Inside `Equate Table (name = value; values, not addresses):`.
    EquateTable,
}

impl SymbolTable {
    /// Parse a sigil/AS `.lst` listing. Pure: no filesystem access — the caller supplies the text.
    ///
    /// Prefers the `Symbol Table` section and falls back to the body lines when no section header is
    /// present (see [`TableSource`]). The `Equate Table` section is recognised and consumed but never
    /// ingested (see the module docs). Unrecognised lines are skipped and counted, never fatal; the only
    /// hard failure is finding no symbols at all.
    pub fn parse(text: &str) -> Result<Self, SymbolParseError> {
        let mut table_syms: Vec<Symbol> = Vec::new();
        let mut body_syms: Vec<Symbol> = Vec::new();
        let mut section = Section::Body;
        let mut declared_count = None;
        let mut declared_unused = None;
        let mut skipped_lines = 0usize;
        let mut equates: Option<EquateSection> = None;
        let mut non_address = NonAddressRows::default();

        for line in text.lines() {
            let t = line.trim_end();
            if t.trim().is_empty() {
                continue;
            }
            let head = t.trim_start();
            if section == Section::Body && head.starts_with("Symbol Table") {
                section = Section::SymbolTable;
                continue;
            }
            if section != Section::EquateTable && head.starts_with("Equate Table") {
                section = Section::EquateTable;
                equates = Some(EquateSection::default());
                continue;
            }
            match section {
                Section::SymbolTable => {
                    // The `-----` rule under the header, and the two footer counts.
                    if is_rule_line(t) {
                        continue;
                    }
                    if let Some(n) = parse_footer(t, "unused symbols") {
                        declared_unused = Some(n);
                        continue;
                    }
                    if let Some(n) = parse_footer(t, "symbols") {
                        declared_count = Some(n);
                        // The footer is the section's end. Everything after it belongs to some other
                        // part of the listing and must not be judged as a malformed row.
                        section = Section::AfterSymbolTable;
                        continue;
                    }
                    // Split on `|` FIRST: AS packs the table two columns wide and sigil terminates its
                    // single column with the same character, so one splitter reads both dialects (see
                    // the module docs). An empty segment is the tail after a trailing `|`, not damage.
                    for part in t.split('|') {
                        if part.trim().is_empty() {
                            continue;
                        }
                        match parse_table_entry(part) {
                            TableEntry::Symbol(s) => table_syms.push(s),
                            TableEntry::NonAddress { unused } => {
                                non_address.rows += 1;
                                non_address.unused += usize::from(unused);
                            }
                            TableEntry::Unrecognised => skipped_lines += 1,
                        }
                    }
                }
                Section::AfterSymbolTable => {
                    // Only the trailing count is still ours; the rest of the listing's tail is not a
                    // symbol list and is neither ingested nor counted as damage. (The `Equate Table`
                    // header is matched above this `match`, so a sigil listing still enters it here.)
                    if let Some(n) = parse_footer(t, "unused symbols") {
                        declared_unused = Some(n);
                    }
                }
                Section::EquateTable => {
                    // Ingested into `e.by_name` and NOWHERE else (§11.36). `equates` is `Some` for the
                    // whole arm — the header set it — but express that with the option rather than an
                    // `unwrap`.
                    let Some(e) = equates.as_mut() else { continue };
                    if is_rule_line(t) {
                        continue;
                    }
                    if let Some(n) = parse_footer(t, "equates") {
                        e.declared = Some(n);
                        continue;
                    }
                    // Recognising the section is not swallowing it: a row that does not match the
                    // `EQU <name> = $<hex>` shape is format drift and still counts as damage.
                    if let Some((name, value)) = parse_equate_row(t) {
                        e.rows += 1;
                        e.by_name.insert(name.to_string(), value);
                    } else {
                        skipped_lines += 1;
                    }
                }
                Section::Body => {
                    // Body lines are only a fallback, and a real AS listing's body is full of source text
                    // that is not a label — a non-match there is normal, so it is not counted as skipped
                    // (that count is about the authoritative half).
                    if let Some(s) = parse_body_line(t) {
                        body_syms.push(s);
                    }
                }
            }
        }

        let (syms, source) = if table_syms.is_empty() {
            (body_syms, TableSource::BodyLines)
        } else {
            (table_syms, TableSource::SymbolTable)
        };
        if syms.is_empty() {
            return Err(SymbolParseError::NoSymbols);
        }
        Ok(Self::build(
            syms,
            source,
            ParseCounts {
                declared_count,
                declared_unused,
                skipped_lines,
                equates,
                non_address,
            },
        ))
    }

    fn build(mut syms: Vec<Symbol>, source: TableSource, counts: ParseCounts) -> Self {
        // Sort on the 24-bit address so nearest-preceding search matches what the bus sees; `name` breaks
        // ties so the ordering (and therefore every lookup answer) is deterministic across runs.
        syms.sort_by(|a, b| a.addr.cmp(&b.addr).then_with(|| a.name.cmp(&b.name)));

        let mut by_name = BTreeMap::new();
        let mut by_demangled: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut by_module: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, s) in syms.iter().enumerate() {
            // A duplicate raw name cannot occur in a real listing (verified: 0 duplicates in `s4.lst`);
            // if one ever does, the later entry in sort order wins, deterministically.
            by_name.insert(s.name.clone(), i);
            by_demangled.entry(s.demangled.clone()).or_default().push(i);
            if let Some(m) = s.scope().module {
                by_module.entry(m.to_string()).or_default().push(i);
            }
        }
        // A demangled spelling shared by two symbols at *different* addresses does not identify a location.
        // Marked per-symbol now that the whole table is known, so `Resolution`'s `Display` can fall back to
        // the unique raw name. Aliases at the *same* address are not ambiguous — either name is correct.
        for idx in by_demangled.values() {
            if idx.len() < 2 {
                continue;
            }
            let a = syms[idx[0]].addr;
            if idx.iter().all(|&i| syms[i].addr == a) {
                continue;
            }
            for &i in idx {
                syms[i].demangled_ambiguous = true;
            }
        }

        // The reverse-lookup index: same order, forward-only rows removed (see `rev`). Built once here so
        // no lookup can forget the rule.
        let rev = syms
            .iter()
            .enumerate()
            .filter(|(_, s)| s.resolves_in_reverse())
            .map(|(i, _)| i)
            .collect();

        Self {
            syms,
            rev,
            by_name,
            by_demangled,
            by_module,
            source,
            declared_count: counts.declared_count,
            declared_unused: counts.declared_unused,
            skipped_lines: counts.skipped_lines,
            equates: counts.equates,
            non_address: counts.non_address,
        }
    }

    /// Every symbol, address-sorted (ties broken by raw name).
    pub fn symbols(&self) -> &[Symbol] {
        &self.syms
    }

    /// How many symbols were parsed.
    pub fn len(&self) -> usize {
        self.syms.len()
    }

    /// Always `false` — [`parse`](Self::parse) rejects an empty listing — but `clippy` asks for it
    /// alongside [`len`](Self::len).
    pub fn is_empty(&self) -> bool {
        self.syms.is_empty()
    }

    /// Which half of the listing this table came from.
    pub fn source(&self) -> TableSource {
        self.source
    }

    /// The count the listing's own `N symbols` footer declares, when present.
    pub fn declared_count(&self) -> Option<usize> {
        self.declared_count
    }

    /// The count the listing's `N unused symbols` footer declares, when present.
    pub fn declared_unused(&self) -> Option<usize> {
        self.declared_unused
    }

    /// Lines inside the `Symbol Table` or `Equate Table` sections that did not parse as a row of that
    /// section. Non-zero means the file is truncated or the format drifted — worth surfacing, never fatal.
    pub fn skipped_lines(&self) -> usize {
        self.skipped_lines
    }

    /// `Symbol Table` rows that were well-formed but carried no address — AS's `ARCHITECTURE`, `DATE`,
    /// `TIME` and friends, whose value is a string or a float. Recognised and consumed, never ingested;
    /// **not** damage, so they are excluded from [`skipped_lines`](Self::skipped_lines) and included in the
    /// footer reconciliation ([`matches_declared_count`](Self::matches_declared_count)). Always 0 on a
    /// sigil listing.
    pub fn non_address_rows(&self) -> usize {
        self.non_address.rows
    }

    /// How many [`non_address_rows`](Self::non_address_rows) carried the `*` unused marker. Needed to
    /// reconcile the `N unused symbols` footer, which counts them: on `sonic.lst` all five are starred, so
    /// 1,568 ingested + 5 here == the declared 1,573.
    pub fn non_address_unused(&self) -> usize {
        self.non_address.unused
    }

    /// How many `EQU <name> = $<hex>` rows the `Equate Table` held, or `None` when the listing has no such
    /// section.
    ///
    /// The **row** count, which is what the `N equates` trailer counts and therefore what
    /// [`matches_declared_equates`](Self::matches_declared_equates) must compare against. It can exceed
    /// the number of distinct names if a listing ever repeats one; it never has.
    pub fn equate_rows(&self) -> Option<usize> {
        self.equates.as_ref().map(|e| e.rows)
    }

    /// **The value of one equate, by exact name** — §11.36's `emulator/lookup_equate` name form, and the
    /// one door the panel's ring ceiling divides by.
    ///
    /// `None` means *this listing does not publish that equate*, which covers both "no `Equate Table` at
    /// all" (every listing sigil emitted before 2026-08-19, and every AS listing) and "the section is
    /// there and the name is not in it". The two are separable by
    /// [`has_equate_table`](Self::has_equate_table); the bus needs the distinction only for its message,
    /// never for its code.
    ///
    /// ⚑ **The value is NOT an address.** 661 of this listing's 724 fall inside the cart window and 67 are
    /// numerically equal to a real label's address; passing one to a bus read is the caller's own
    /// authority, and nothing here masks, widens or validates it as a location.
    pub fn equate_value(&self, name: &str) -> Option<u64> {
        self.equates.as_ref()?.by_name.get(name).copied()
    }

    /// **Every equate whose name starts with `prefix`**, name-ordered, unbounded — §11.36's prefix form.
    ///
    /// The **bound is the caller's**, exactly as it is for [`with_prefix`](Self::with_prefix): a policy
    /// ceiling belongs to the server that has to declare `truncated`, not to the parser. An empty result
    /// is a real answer (no equate starts with that prefix), not an error.
    ///
    /// A range query on the ordered map, so it costs `O(log n + matches)` rather than a scan of 724 rows.
    pub fn equates_with_prefix(&self, prefix: &str) -> Vec<(&str, u64)> {
        let Some(e) = self.equates.as_ref() else {
            return Vec::new();
        };
        e.by_name
            .range(prefix.to_string()..)
            .take_while(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.as_str(), *v))
            .collect()
    }

    /// Does this listing carry an `Equate Table` section at all?
    ///
    /// Separates *"the listing publishes no equates"* from *"the listing publishes equates and not this
    /// one"* — a distinction a reader debugging a missing constant needs and a `None` from
    /// [`equate_value`](Self::equate_value) cannot carry.
    pub fn has_equate_table(&self) -> bool {
        self.equates.is_some()
    }

    /// How many distinct equate **names** this listing publishes. 0 when there is no section.
    pub fn equate_count(&self) -> usize {
        self.equates.as_ref().map_or(0, |e| e.by_name.len())
    }

    /// The count the `Equate Table`'s own `N equates` trailer declares. `None` when there is no such
    /// section, **or** when the section is there but its trailer is not — which is what truncation looks
    /// like, and why [`is_intact`](Self::is_intact) treats the two the same.
    pub fn declared_equates(&self) -> Option<usize> {
        self.equates.as_ref().and_then(|e| e.declared)
    }

    /// Did the `Equate Table` trailer agree with the rows we recognised? `None` when the listing has no
    /// `Equate Table` at all — which is not a failure, just nothing to check.
    pub fn matches_declared_equates(&self) -> Option<bool> {
        self.equates.as_ref().map(|e| e.declared == Some(e.rows))
    }

    /// Did we account for exactly as many rows as the footer promised? `None` when there is no footer to
    /// check against — which is itself a damage signal, so prefer [`is_intact`](Self::is_intact) for a
    /// yes/no verdict.
    ///
    /// The comparison is against ingested symbols **plus** [`non_address_rows`](Self::non_address_rows),
    /// because AS's footer counts its string and float pseudo-symbols as symbols. On a sigil listing the
    /// second term is 0 and this is the plain count it always was.
    pub fn matches_declared_count(&self) -> Option<bool> {
        self.declared_count
            .map(|n| n == self.syms.len() + self.non_address.rows)
    }

    /// Does this listing look like a whole, undamaged file? True only when it has the `Symbol Table`
    /// section, its `N symbols` footer, a count that matches what we parsed, no unrecognised rows, and —
    /// if it carries an `Equate Table` at all — an `N equates` trailer that agrees with the rows we saw.
    ///
    /// That last condition is what makes consuming the equate rows silently safe. The trailer is the
    /// section's own checksum; without checking it, a truncated tail or a drifted row shape would vanish
    /// into the same silence that hides real damage. A listing with **no** `Equate Table` is unaffected —
    /// the condition is vacuous, not failed — which covers every listing sigil emitted before 2026-08-19.
    ///
    /// Truncation is the case this exists for, and it is nastier than it looks. A half-written listing
    /// usually loses its footer *along with* its tail, so `matches_declared_count()` returns `None` rather
    /// than `Some(false)` and a naive check waves it through; worse, cutting the whole `Symbol Table`
    /// section silently demotes parsing to the body-line fallback while still producing thousands of valid
    /// symbols. Measured on a truncated real `s4.lst`: at a 90% cut, 2,023 addresses resolve to a
    /// *different but still plausible* name. Every one of those four conditions catches a cut this
    /// coarse-grained count comparison alone would miss.
    ///
    /// This is deliberately about the **file**, not about which ROM it describes — see
    /// [`validate_against_rom`](Self::validate_against_rom) for that. Callers should combine the two: a
    /// listing whose ROM binding is `Indeterminate` *and* which is not intact should be refused, because
    /// "no fingerprint" may simply be the fingerprint symbol having fallen off the end.
    pub fn is_intact(&self) -> bool {
        self.source == TableSource::SymbolTable
            && self.matches_declared_count() == Some(true)
            && self.skipped_lines == 0
            && self.matches_declared_equates() != Some(false)
    }

    /// Exact lookup by the **raw** mangled name (`$engine.boot$EntryPoint$wait_dma`).
    pub fn by_name(&self, name: &str) -> Option<&Symbol> {
        self.by_name.get(name).map(|&i| &self.syms[i])
    }

    /// Exact lookup by the **demangled** name (`EntryPoint.wait_dma`). Returns every match, because
    /// `Parent.local` is not unique across modules; address-sorted, and empty when nothing matches.
    pub fn by_demangled(&self, name: &str) -> Vec<&Symbol> {
        self.by_demangled
            .get(name)
            .map(|v| v.iter().map(|&i| &self.syms[i]).collect())
            .unwrap_or_default()
    }

    /// Name → address, trying the raw spelling first and then the demangled one. Returns the **24-bit**
    /// bus address. Ambiguous demangled names yield `None` rather than an arbitrary pick — silently
    /// choosing one of several `Parent.local` collisions is exactly the kind of confidently-wrong answer
    /// this module exists to prevent.
    pub fn address_of(&self, name: &str) -> Option<u32> {
        if let Some(s) = self.by_name(name) {
            return Some(s.addr);
        }
        match self.by_demangled.get(name) {
            Some(v) if v.len() == 1 => Some(self.syms[v[0]].addr),
            _ => None,
        }
    }

    /// Every symbol whose address is exactly `addr` (masked to 24 bits). More than one is normal — sigil
    /// emits a label per source line, so two consecutive labels with no code between them share an address
    /// (`$engine.boot$EntryPoint$wait_dma` and `…$warm_boot` are both `$214`).
    ///
    /// This is the addr→name direction, so forward-only symbols are excluded
    /// ([`Symbol::resolves_in_reverse`]) — an exact hit on a constant is the most confident wrong answer of
    /// all. Address-sorted, and empty when nothing matches.
    pub fn symbols_at(&self, addr: u32) -> Vec<&Symbol> {
        let a = addr & BUS_ADDR_MASK;
        let lo = self.rev.partition_point(|&i| self.syms[i].addr < a);
        let hi = self.rev.partition_point(|&i| self.syms[i].addr <= a);
        self.rev[lo..hi].iter().map(|&i| &self.syms[i]).collect()
    }

    /// **The workhorse.** Nearest symbol at or before `addr`, plus the displacement — "PC is at
    /// `EntryPoint`+$1A".
    ///
    /// `addr` may be given in either spelling (`0xFFFF_8CFA` or `0xFF_8CFA`); it is masked to 24 bits
    /// first. Resolution never crosses an [`AddrSpace`] boundary: an address in work RAM below the first
    /// RAM symbol returns `None` instead of the last ROM symbol plus a ~15 MB displacement, and an address
    /// in an unmapped gap resolves to nothing at all.
    ///
    /// When several symbols share the winning address, the last in `(addr, name)` order is returned —
    /// deterministic, and [`symbols_at`](Self::symbols_at) exposes the full set of aliases.
    ///
    /// Forward-only symbols never answer here ([`Symbol::resolves_in_reverse`]): an AS `-` row may hold a
    /// bare constant, and nearest-preceding search over constants is how a low address acquires a
    /// confident, wrong name.
    pub fn resolve(&self, addr: u32) -> Option<Resolution<'_>> {
        let a = addr & BUS_ADDR_MASK;
        let idx = self.rev.partition_point(|&i| self.syms[i].addr <= a);
        let sym = &self.syms[self.rev[idx.checked_sub(1)?]];
        if AddrSpace::of(sym.addr) != AddrSpace::of(a) {
            return None;
        }
        Some(Resolution {
            symbol: sym,
            displacement: a - sym.addr,
        })
    }

    /// [`resolve`](Self::resolve), but rejecting an answer further than `max_displacement` past the
    /// symbol. Use this when a wrong-but-plausible name is worse than no name — e.g. annotating a PC that
    /// may have run off into unmapped space, where the nearest preceding label can be thousands of bytes back.
    pub fn resolve_within(&self, addr: u32, max_displacement: u32) -> Option<Resolution<'_>> {
        self.resolve(addr)
            .filter(|r| r.displacement <= max_displacement)
    }

    /// Every symbol whose **raw** name starts with `prefix`, address-sorted. A range query on the ordered
    /// index, so it costs `O(log n + matches)`.
    pub fn with_prefix(&self, prefix: &str) -> Vec<&Symbol> {
        let mut idx = prefix_indexes(&self.by_name, prefix, std::slice::from_ref);
        idx.sort_unstable();
        idx.into_iter().map(|i| &self.syms[i]).collect()
    }

    /// Every symbol whose **demangled** name starts with `prefix` — the search a human types
    /// (`Player_`, `EntryPoint.`), address-sorted and de-duplicated.
    pub fn with_demangled_prefix(&self, prefix: &str) -> Vec<&Symbol> {
        let mut idx = prefix_indexes(&self.by_demangled, prefix, Vec::as_slice);
        idx.sort_unstable();
        idx.dedup();
        idx.into_iter().map(|i| &self.syms[i]).collect()
    }

    /// Every `.emp` module path seen, sorted. The top level of the scope tree.
    pub fn modules(&self) -> Vec<&str> {
        self.by_module.keys().map(String::as_str).collect()
    }

    /// Every symbol belonging to one module, address-sorted. The second level of the scope tree.
    pub fn symbols_in_module(&self, module: &str) -> Vec<&Symbol> {
        self.by_module
            .get(module)
            .map(|v| v.iter().map(|&i| &self.syms[i]).collect())
            .unwrap_or_default()
    }

    /// Decide whether this listing describes `rom` — the guard against trap 4.
    ///
    /// Mechanism (chosen after investigating the alternatives, all verified against real images): every
    /// sigil-built Aeon ROM carries a `deb2` symbol appendix appended at `EndOfRom`, and `EndOfRom` is a
    /// symbol *in the listing*. So the listing names an offset, and the ROM either has the magic there or
    /// it does not. No new format needs decoding — we read two bytes.
    ///
    /// Verified in both directions: `s4.bin` at `s4.debug.lst`'s `EndOfRom` (`$A30B0`) reads `43 0b …`,
    /// and `s4.debug.bin` at `s4.lst`'s `EndOfRom` (`$A11F0`) reads all zeros. The shape cross that
    /// silently mis-resolves 92.6% of shared symbols is caught.
    ///
    /// # Known residual limit — this is a strong filter, not a proof
    ///
    /// `demo.lst` and `demo.debug.lst` **both** declare `EndOfRom : 11224`. Two genuinely different builds
    /// (1,400 vs 1,621 symbols; 1,197 shared symbols at differing addresses) therefore agree on the one
    /// offset we can probe, so this check would pass either listing against either image. Nothing available
    /// today binds a listing to an image with certainty short of decoding the appendix or re-running
    /// `convsym`; closing it properly wants a producer-side change in sigil (emit the built image's
    /// checksum, or a hash, into a sidecar beside the `.lst`). Callers should treat [`RomBinding::Match`]
    /// as "not obviously wrong", not as "proven right".
    ///
    /// # Listings that describe an appendix-less ROM
    ///
    /// A stock AS disassembly has no `deb2` appendix at all, and ends with `RomEndLoc: dc.l EndOfRom-1` —
    /// which puts `EndOfRom` at **exactly** the image length (verified: `s1disasm`'s `$86978` == its built
    /// ROM's 551,288 bytes). That exact equality is treated as a no-appendix marker and answers
    /// [`Indeterminate::EndOfRomIsImageEnd`], not [`BindingFault::EndOfRomOutOfRange`]: a symbol one past
    /// the last byte cannot carry the magic and cannot be a mismatched in-range offset, so there is
    /// genuinely nothing to check rather than something that failed a check.
    ///
    /// **The wrong-listing guard is untouched.** An `EndOfRom` that lands *inside* the image without the
    /// magic there is still `Mismatch(NoAppendixMagic)` — the check that catches the build-shape cross
    /// where 92.6% of shared symbols resolve to a different address — and so is one past the end, or one
    /// byte short of it with no room for the two magic bytes. Only the exact `EndOfRom == rom.len()` case
    /// moved, and it moved from a fault to "cannot tell", never to a match.
    pub fn validate_against_rom(&self, rom: &[u8]) -> RomBinding {
        let Some(end) = self.address_of(END_OF_ROM_SYMBOL) else {
            return RomBinding::Indeterminate(Indeterminate::NoEndOfRomSymbol);
        };
        let off = end as usize;
        // The no-appendix marker (ruled 2026-08-19). A stock AS disassembly ends `RomEndLoc: dc.l
        // EndOfRom-1`, which puts `EndOfRom` at exactly the image length — measured on `s1disasm`:
        // `EndOfRom = $86978 = 551,288 = sonic.bin's size to the byte`. That is not a fault, it is a
        // listing that describes a ROM with no appendix, and it is safe to distinguish from one *because*
        // the equality is exact: an offset one past the last byte can carry no magic to check, and it
        // cannot be an in-range offset whose magic came out wrong. So it downgrades to Indeterminate —
        // "accepted unverified, with the caveat" under the frontend's existing load policy.
        //
        // Everything else is UNCHANGED, and deliberately: an in-range `EndOfRom` with no `de b2` at it is
        // still a positive `Mismatch`, which is the guard that catches the shape cross where 92.6% of
        // shared symbols resolve to a different address. `off > rom.len()`, and `off == rom.len() - 1`
        // (one byte, no room for the magic), both stay faults too — only the exact equality is the marker.
        if off == rom.len() {
            return RomBinding::Indeterminate(Indeterminate::EndOfRomIsImageEnd {
                rom_len: rom.len(),
            });
        }
        let Some(head) = rom.get(off..off + DEB2_MAGIC.len()) else {
            return RomBinding::Mismatch(BindingFault::EndOfRomOutOfRange {
                end_of_rom: end,
                rom_len: rom.len(),
            });
        };
        if head != DEB2_MAGIC {
            return RomBinding::Mismatch(BindingFault::NoAppendixMagic {
                offset: end,
                found: [head[0], head[1]],
            });
        }
        let len = rom.len() - off;
        if len < DEB2_MIN_LEN {
            return RomBinding::Mismatch(BindingFault::AppendixTooSmall { offset: end, len });
        }
        RomBinding::Match {
            appendix_offset: end,
            appendix_len: len,
        }
    }
}

/// Collect the indexes of every ordered-map entry whose key starts with `prefix`, via a range query.
/// `expand` turns one entry's value into the indexes it contributes (one for `by_name`, many for
/// `by_demangled`).
fn prefix_indexes<'m, V, F>(map: &'m BTreeMap<String, V>, prefix: &str, expand: F) -> Vec<usize>
where
    F: Fn(&'m V) -> &'m [usize],
{
    let mut out = Vec::new();
    for (k, v) in map.range(prefix.to_string()..) {
        if !k.starts_with(prefix) {
            break;
        }
        out.extend_from_slice(expand(v));
    }
    out
}

/// The `-----` rule the emitter draws under each section header.
fn is_rule_line(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('-') && t.chars().all(|c| c == '-')
}

/// Parse an `Equate Table` row — `EQU <name> = $<hex>` — into its name and its **value**.
///
/// The value goes into [`EquateSection::by_name`] and nowhere else. It is deliberately *not* masked with
/// [`BUS_ADDR_MASK`] and deliberately *not* range-checked against the cart window, because §11.36 pins it
/// as a plain integer that **is not an address**: masking would be this module quietly asserting the
/// opposite. Shape verified against the real listings: every row is exactly four whitespace tokens, the
/// value always carries the `$`, and the widest value seen across `fixtures/aeon/s4.debug.lst` and aeon's
/// live `s4.debug.lst` is 8 hex digits (`$FFFFFF00`).
///
/// **A value too wide for `u64` makes the row unrecognised, and that is a narrowing.** The predicate this
/// replaces accepted any run of hex digits, because it stored nothing; a row we cannot represent is now a
/// row we cannot answer for, so it is reported as damage (`skipped_lines`, and the `N equates` trailer
/// then disagrees) rather than counted as a row whose value silently is not there. 17 hex digits have
/// never been seen in either dialect.
fn parse_equate_row(line: &str) -> Option<(&str, u64)> {
    let tok: Vec<&str> = line.split_whitespace().collect();
    if tok.len() != 4 || tok[0] != "EQU" || tok[1].is_empty() || tok[2] != "=" {
        return None;
    }
    let hex = tok[3].strip_prefix('$')?;
    if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some((tok[1], u64::from_str_radix(hex, 16).ok()?))
}

/// Parse `   2129 symbols` / `    0 unused symbols` / `   682 equates`. `suffix` is matched on the whole
/// remainder so
/// `"symbols"` does not swallow the `"unused symbols"` line (the caller tries the longer one first).
fn parse_footer(line: &str, suffix: &str) -> Option<usize> {
    let t = line.trim();
    let rest = t.strip_suffix(suffix)?;
    rest.trim_end().parse::<usize>().ok()
}

/// What one `|`-delimited segment of a `Symbol Table` line turned out to be.
enum TableEntry {
    /// A symbol with an address.
    Symbol(Symbol),
    /// A well-formed row whose value is not an address — AS metadata like `DATE : "08/19/2026" -`.
    /// Consumed and counted, never ingested; see [`NonAddressRows`].
    NonAddress { unused: bool },
    /// Not a row of this section at all. Counted as damage.
    Unrecognised,
}

/// Parse one `|`-delimited segment of a `Symbol Table` line: `[*]NAME : VALUE <C|->`.
///
/// The caller has already split the line on `|`, which is what makes one parser read both dialects — AS's
/// two-per-line packing and sigil's one-per-line-with-a-trailing-bar (see the module docs). The `*` is
/// emitted in the leading column (no separating space) when a symbol is unused; a used symbol gets a space
/// there instead. Tokenising on whitespace after peeling the marker tolerates any column alignment.
///
/// The shape is `NAME`, `:`, one-or-more value tokens, then the type letter — the value is allowed to span
/// tokens because AS quotes strings that contain spaces (`TIME : "11:35:59 PM" -`). A value is an
/// **address** only when it is a single all-hex token that fits in `u64`. Requiring the terminating type
/// letter is what keeps a real AS listing's source text from being mistaken for symbols now that the token
/// count is no longer fixed.
///
/// **The type letter decides what an unparseable value means**, and the asymmetry is the whole point. A
/// `-` row is an equate, and an equate is legitimately a string or a float, so a non-address there is
/// normal output ([`TableEntry::NonAddress`]). A `C` row is an address in the assembled image by
/// definition, so a value that is not one is corruption and stays damage — which is what keeps
/// `EndOfRom : ZZZZ C` a detected truncation rather than a silently-tolerated row.
fn parse_table_entry(part: &str) -> TableEntry {
    let t = part.trim_start();
    let (unused, rest) = match t.strip_prefix('*') {
        Some(r) => (true, r),
        None => (false, t),
    };
    let tok: Vec<&str> = rest.split_whitespace().collect();
    // NAME, ':', at least one value token, and the type letter.
    if tok.len() < 4 || tok[1] != ":" {
        return TableEntry::Unrecognised;
    }
    let kind = match tok[tok.len() - 1] {
        "C" => SymbolKind::Code,
        "-" => SymbolKind::Equate,
        _ => return TableEntry::Unrecognised,
    };
    // A value that is not an address: normal output on an equate row, corruption on a code row.
    let not_an_address = || match kind {
        SymbolKind::Equate => TableEntry::NonAddress { unused },
        SymbolKind::Code => TableEntry::Unrecognised,
    };
    let [v] = &tok[2..tok.len() - 1] else {
        return not_an_address();
    };
    // `u64`, not `u32`: AS spells a RAM address sign-extended to 48 bits (`FFFFFFFFFFFFD000`).
    // `from_str_radix` would also accept a leading `+`, so the digits are checked explicitly.
    if v.is_empty() || !v.bytes().all(|b| b.is_ascii_hexdigit()) {
        return not_an_address();
    }
    let Ok(raw) = u64::from_str_radix(v, 16) else {
        return not_an_address();
    };
    TableEntry::Symbol(make_symbol(tok[0].to_string(), raw, kind, unused))
}

/// Parse one body line: `(depth) IDX/HEXADDR :        NAME:`.
///
/// Used only when a listing has no `Symbol Table` section. Deliberately strict — exactly four tokens, a
/// parenthesised depth, an `IDX/HEX` pair, a bare `:`, and a name ending in `:` — so that an ordinary AS
/// listing's code lines (`(0) 5/200 : 4E71   nop`) do not parse as symbols.
fn parse_body_line(line: &str) -> Option<Symbol> {
    let tok: Vec<&str> = line.split_whitespace().collect();
    if tok.len() != 4 || tok[2] != ":" {
        return None;
    }
    if !(tok[0].starts_with('(') && tok[0].ends_with(')')) {
        return None;
    }
    let (_idx, hex) = tok[1].split_once('/')?;
    // `u64` for the same reason the table rows use it: an AS body line spells RAM sign-extended.
    let raw_addr = u64::from_str_radix(hex, 16).ok()?;
    let name = tok[3].strip_suffix(':')?;
    if name.is_empty() {
        return None;
    }
    // Body lines carry neither the `*` marker nor the type column; `Code`/not-unused is the only honest
    // reading, and `TableSource::BodyLines` tells the caller those two fields are unavailable, not observed.
    Some(make_symbol(
        name.to_string(),
        raw_addr,
        SymbolKind::Code,
        false,
    ))
}

/// Is `part` a synthetic compiler block scope (`asm0`, `asm1`, …)? Mirrors sigil's own
/// `is_asm_block_scope`: `asm` alone and `asmName` are real names and must not match.
fn is_asm_block_scope(part: &str) -> bool {
    part.strip_prefix("asm")
        .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
}

/// Split a possibly-mangled name into its scope levels.
///
/// **The module is located by the dot, not by position.** The obvious reading of the mangling —
/// "`$<module>$<Parent>$<local>`, so the module is the first component" — is wrong for a shape that occurs
/// in real listings: a label emitted inside a macro carries the macro instance *outside* the module, as
/// `$diag2$engine.bg_anim$raise`. Taking the first component there yields a phantom module `diag2`, and on
/// the real `s4.debug.lst` that misfiles 125 symbols and invents 34 phantom modules alongside the 51 real
/// ones. (The release `s4.lst` contains no macro-scoped labels at all, which is exactly why a positional
/// rule looks correct until you open a debug build.)
///
/// A `.emp` module path always contains a `.` and no other level ever does. Verified across all six real
/// listings (`s4`, `s4.debug`, `demo`, `demo.debug`, `s4.stress`, `s4.stressart`): every mangled name has
/// **exactly one** dotted component, in one of three arrangements —
///
/// | shape | example | dotted |
/// |---|---|---|
/// | `$module$Parent$local` | `$engine.boot$EntryPoint$wait_dma` | 1st |
/// | `$outer$module$local` | `$diag2$engine.bg_anim$raise`, `__align$engine.boot_data$0` | 2nd |
/// | `$outer$module$Parent$local` | `__offsets$games.sonic4.sonic_anims$Ani_Sonic$Walk` | 2nd |
///
/// so "the component with the dot is the module" resolves all three without guessing. Anything before it is
/// [`Scope::outer`]; the last component is always the label; anything between module and label is the
/// parent. A name with no dotted component falls back to the positional reading rather than giving up.
fn scope_parts(name: &str) -> Scope<'_> {
    let plain = Scope {
        outer: None,
        module: None,
        parent: None,
        local: name,
    };
    if !name.contains('$') {
        return plain;
    }
    let parts: Vec<&str> = name.split('$').filter(|p| !p.is_empty()).collect();
    let Some((&local, head)) = parts.split_last() else {
        return plain;
    };
    // The module is the one dotted component; without one, fall back to "second-to-last is the parent".
    match head.iter().position(|p| p.contains('.')) {
        Some(m) => Scope {
            // `then`, not `then_some`: the latter evaluates `head[m - 1]` even when `m == 0`.
            outer: (m > 0).then(|| head[m - 1]),
            module: Some(head[m]),
            parent: head.get(m + 1..).and_then(|t| t.last()).copied(),
            local,
        },
        None => Scope {
            outer: head.len().checked_sub(2).and_then(|i| head.get(i)).copied(),
            module: None,
            parent: head.last().copied(),
            local,
        },
    }
}

/// The readable spelling of a mangled name: **the last two `$` components, joined with a dot** — sigil's
/// own `demangle_symbols` rule, reproduced deliberately rather than derived from [`scope_parts`].
///
/// Matching sigil matters because sigil applies this same rule before handing names to `convsym`, so these
/// are the strings baked into the ROM's `deb2` appendix and shown by the on-target MD Debugger. A name
/// printed here therefore reads identically to one printed on the Genesis screen. That is why
/// `$diag2$engine.bg_anim$raise` demangles to `engine.bg_anim.raise` (module + label) even though
/// [`scope_parts`] correctly reports its module as `engine.bg_anim` and its parent as none: the display
/// name follows the debugger, the scope tree follows the truth.
///
/// Divergence from sigil, all on degenerate forms that occur in **no** real listing (verified across all
/// six): sigil *drops* a single-component mangled name (`$only`) and a bare `$`; we keep them as their
/// remaining text rather than losing an address.
fn demangle(name: &str) -> String {
    if !name.contains('$') {
        return name.to_string();
    }
    let parts: Vec<&str> = name.split('$').filter(|p| !p.is_empty()).collect();
    match parts.len() {
        0 => name.to_string(),
        1 => parts[0].to_string(),
        n => format!("{}.{}", parts[n - 2], parts[n - 1]),
    }
}

/// Build a [`Symbol`], deriving the demangled name, the 24-bit address, and the synthetic flag.
/// `demangled_ambiguous` is not knowable per-symbol and is filled in later by [`SymbolTable::build`].
///
/// `raw` is the listing's value at full width. [`Symbol::raw_addr`] keeps its low 32 bits (which turns AS's
/// sign-extended `FFFFFFFFFFFFD000` into the conventional `0xFFFF_D000`) and [`Symbol::addr`] masks to the
/// 24 lines the 68000 actually drives — so `addr == raw_addr & BUS_ADDR_MASK` regardless of the spelling.
fn make_symbol(name: String, raw: u64, kind: SymbolKind, unused: bool) -> Symbol {
    let parts: Vec<&str> = name.split('$').filter(|p| !p.is_empty()).collect();
    // `parts.first()`, not `name.starts_with` — a plain symbol called `__alignment` is a real name.
    let is_synthetic =
        parts.first() == Some(&"__align") || parts.iter().copied().any(is_asm_block_scope);
    Symbol {
        demangled: demangle(&name),
        raw_addr: raw as u32,
        addr: (raw & BUS_ADDR_MASK as u64) as u32,
        kind,
        unused,
        is_synthetic,
        demangled_ambiguous: false,
        name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A miniature but format-faithful listing: all three sections, mangled and plain names, an
    /// `__align` pad, an `asm<N>` block scope, an aliased address, an 8-hex `FFFFxxxx` RAM block, an
    /// unused `*` row, an `-` equate, the two symbol footers, and an `Equate Table` carrying **both**
    /// kinds of collision the §11.36 ruling turns on: a name (`Player_1`) that collides with a real
    /// label, and a **value** (`ART_ENTRY = $200`) that is exactly the address of one (`EntryPoint`).
    const FIXTURE: &str = "\
(0) 1/200 :        EntryPoint:
(0) 2/214 :        $engine.boot$EntryPoint$wait_dma:
(0) 3/214 :        $engine.boot$EntryPoint$warm_boot:
(0) 4/2B8 :        $engine.boot$asm1$wait_z80:
(0) 5/2C0 :        $diag2$engine.bg_anim$raise:
(0) 6/2C8 :        $diag3$engine.bg_anim$raise:
(0) 7/1BEA :        __align$engine.boot_data$0:
(0) 8/A11F0 :        EndOfRom:
(0) 9/FFFF8CFA :        Player_1:
(0) 10/FFFFB790 :        Character_ID:
  Symbol Table (* = unused):
  --------------------------

 EntryPoint : 200 C |
 $engine.boot$EntryPoint$wait_dma : 214 C |
 $engine.boot$EntryPoint$warm_boot : 214 C |
 $engine.boot$asm1$wait_z80 : 2B8 C |
 $diag2$engine.bg_anim$raise : 2C0 C |
 $diag3$engine.bg_anim$raise : 2C8 C |
*__align$engine.boot_data$0 : 1BEA C |
 EndOfRom : A11F0 C |
 Player_1 : FFFF8CFA C |
 Character_ID : FFFFB790 - |

   10 symbols
    1 unused symbols

  Equate Table (name = value; values, not addresses):
  ---------------------------------------------------

EQU AF_BACK = $000000FE
EQU ART_ENTRY = $00000200
EQU Player_1 = $00000001
EQU zone_count = $0000000C

    4 equates
";

    fn table() -> SymbolTable {
        SymbolTable::parse(FIXTURE).expect("fixture parses")
    }

    /// The `Equate Table` sigil started emitting on 2026-08-19 is a **known** section, not damage: its
    /// header, rule, rows and trailer are all consumed, so a healthy listing still reports zero skipped
    /// lines and stays [`SymbolTable::is_intact`]. Negative control for deleting the section recognition.
    #[test]
    fn equate_table_is_recognised_and_costs_no_damage() {
        let t = table();
        assert_eq!(t.skipped_lines(), 0, "the Equate Table is not damage");
        assert!(t.is_intact(), "a listing with equates is still whole");
        assert_eq!(t.len(), 10, "equates must not inflate the symbol count");
        assert_eq!(t.declared_count(), Some(10));
        assert_eq!(t.equate_rows(), Some(4));
        assert_eq!(t.declared_equates(), Some(4));
        assert_eq!(t.matches_declared_equates(), Some(true));
    }

    /// ⚑ **THE SAFETY PROPERTY §11.36 ADOPTED OPTION A FOR: an equate never enters `addr→name`.**
    ///
    /// Equates are **values, not addresses** (sigil's own pin, and now the contract's: *"NOT an address
    /// even when it falls inside the cart window"*). Since CR-M they are readable — so a test that only
    /// checked they answer nothing would now be checking the wrong thing. This checks the division:
    ///
    /// 1. the equate door **does** answer, by exact name and by prefix, so clause 2 is about separation
    ///    and not about an empty map (without this the whole test would pass on a parser that dropped the
    ///    section again, which is exactly the regression it is here to catch);
    /// 2. not one equate name reaches any **symbol** index — `by_name`, `by_demangled`, `address_of`, or
    ///    either prefix search;
    /// 3. and the case the ruling turns on: **`ART_ENTRY = $200` is the address of `EntryPoint`**, and
    ///    `$200` still resolves to `EntryPoint`, is still carried by exactly one symbol, and the reverse
    ///    direction never once names the equate. On the real listing 67 equates have a value equal to a
    ///    label's address and 661 of 724 fall inside the cart window, so this is the common case, not a
    ///    contrived one.
    ///
    /// Negative control for "simplifying" the two maps into one with a `kind` flag: a shared table stays
    /// green here only while every reverse path remembers to test the flag, and clause 3 goes red the
    /// moment one forgets.
    #[test]
    fn equates_are_not_addressable_in_either_direction() {
        let t = table();

        // 1. The door is open — otherwise every assertion below is vacuous.
        assert_eq!(t.equate_value("AF_BACK"), Some(0xFE));
        assert_eq!(t.equate_value("ART_ENTRY"), Some(0x200));
        assert_eq!(t.equate_count(), 4);
        assert!(t.has_equate_table());
        assert_eq!(
            t.equates_with_prefix("A"),
            vec![("AF_BACK", 0xFE), ("ART_ENTRY", 0x200)],
            "the prefix form is name-ordered and bounded by the caller, not here"
        );

        // 2. name → address: absent entirely from every SYMBOL index.
        for name in ["AF_BACK", "ART_ENTRY", "zone_count"] {
            assert!(t.by_name(name).is_none(), "{name} leaked into by_name");
            assert!(
                t.by_demangled(name).is_empty(),
                "{name} leaked into by_demangled"
            );
            assert_eq!(t.address_of(name), None, "{name} resolved to an address");
            assert!(
                t.with_prefix(name).is_empty(),
                "{name} leaked into prefixes"
            );
            assert!(
                t.with_demangled_prefix(name).is_empty(),
                "{name} leaked into demangled prefixes"
            );
        }
        // A name shared by an equate and a real label still answers with the *label*, untouched — and the
        // equate door still answers with the equate. Two namespaces, two answers, neither shadowing the
        // other.
        assert_eq!(t.address_of("Player_1"), Some(0x00FF_8CFA));
        assert_eq!(t.by_demangled("Player_1").len(), 1);
        assert_eq!(t.equate_value("Player_1"), Some(1));

        // 3. THE ONE THAT MATTERS. `ART_ENTRY`'s value IS `EntryPoint`'s address.
        assert_eq!(
            t.equate_value("ART_ENTRY"),
            t.address_of("EntryPoint").map(u64::from),
            "the fixture must keep the collision this clause exists to test"
        );
        let at = t.resolve(0x200).expect("EntryPoint is at $200");
        assert_eq!(
            at.symbol.name, "EntryPoint",
            "the equate whose VALUE is $200 must not win the addr->name query"
        );
        assert_eq!(
            t.symbols_at(0x200).len(),
            1,
            "exactly one symbol sits at $200; an ingested equate would make it two"
        );

        // address → name: an equate's *value* is not a location. `$FE`, `$1` and `$C` sit below every real
        // symbol, so nearest-preceding must find nothing rather than inventing a name for a constant.
        for value in [0x0000_00FEu32, 0x0000_0001, 0x0000_000C] {
            assert!(
                t.symbols_at(value).is_empty(),
                "a symbol landed on ${value:X}"
            );
            assert!(
                t.resolve(value).is_none(),
                "${value:X} resolved to a name — an equate value became an address"
            );
        }
        // The sweep the three spot-checks cannot do: over EVERY equate, no reverse lookup at its value
        // ever names it. Stated this way rather than "no symbol is called X" because `Player_1` is
        // legitimately both a label and an equate name — the name collision is not the hazard, the
        // VALUE collision is, and this is the property that distinguishes them.
        let all = t.equates_with_prefix("");
        assert_eq!(all.len(), 4, "the sweep's left side is not empty");
        for (name, value) in all {
            let Ok(addr) = u32::try_from(value) else {
                continue;
            };
            if let Some(r) = t.resolve(addr) {
                assert_ne!(
                    r.symbol.name, name,
                    "resolving ${addr:X} named the equate `{name}` — an equate reached addr->name"
                );
            }
            for s in t.symbols_at(addr) {
                assert_ne!(
                    s.name, name,
                    "symbols_at(${addr:X}) carried the equate `{name}`"
                );
            }
        }
    }

    /// The `N equates` trailer is the section's own checksum. Consuming the rows silently is only safe
    /// while that check holds, so a trailer that disagrees with the rows we recognised — or one that is
    /// missing entirely, which is what truncation looks like — makes the listing not-intact.
    #[test]
    fn an_equate_trailer_that_disagrees_with_the_rows_seen_is_damage() {
        let wrong = FIXTURE.replace("    4 equates", "    5 equates");
        let t = SymbolTable::parse(&wrong).expect("still parses");
        assert_eq!(t.equate_rows(), Some(4));
        assert_eq!(t.declared_equates(), Some(5));
        assert_eq!(t.matches_declared_equates(), Some(false));
        assert!(!t.is_intact(), "a miscounted Equate Table is damage");
        // The symbol half is untouched — the anomaly is reported, not spread.
        assert_eq!(t.len(), 10);
        assert_eq!(t.skipped_lines(), 0);

        let truncated = FIXTURE.replace("    4 equates\n", "");
        let t = SymbolTable::parse(&truncated).expect("still parses");
        assert_eq!(t.declared_equates(), None);
        assert_eq!(t.matches_declared_equates(), Some(false));
        assert!(!t.is_intact(), "a trailer-less Equate Table is damage");

        // A listing with no Equate Table at all is unaffected: the condition is vacuous, not failed.
        let none = FIXTURE.split("  Equate Table").next().unwrap();
        let t = SymbolTable::parse(none).expect("pre-2026-08-19 listing parses");
        assert_eq!(t.equate_rows(), None);
        assert_eq!(t.matches_declared_equates(), None);
        assert!(t.is_intact(), "a listing without equates is still whole");
    }

    /// Recognising the section is not the same as swallowing it. A row inside the Equate Table that does
    /// not match `EQU <name> = $<hex>` is format drift and is still counted as a skipped line.
    #[test]
    fn an_unrecognised_row_inside_the_equate_table_is_still_skipped() {
        let drifted = FIXTURE.replace(
            "EQU zone_count = $0000000C",
            "EQUATE zone_count -> 0000000C",
        );
        let t = SymbolTable::parse(&drifted).expect("still parses");
        assert_eq!(t.skipped_lines(), 1, "drifted equate row must be reported");
        assert_eq!(t.equate_rows(), Some(3));
        assert_eq!(
            t.equate_value("zone_count"),
            None,
            "a row we could not parse must not leave a half-ingested value behind"
        );
        assert!(!t.is_intact());
        assert_eq!(t.len(), 10, "the symbol half is unharmed");
    }

    #[test]
    fn parses_both_sections_and_prefers_the_symbol_table() {
        let t = table();
        assert_eq!(t.source(), TableSource::SymbolTable);
        assert_eq!(t.len(), 10);
        assert_eq!(t.declared_count(), Some(10));
        assert_eq!(t.declared_unused(), Some(1));
        assert_eq!(t.matches_declared_count(), Some(true));
        assert_eq!(t.skipped_lines(), 0);
    }

    #[test]
    fn body_lines_are_the_fallback_when_no_section_exists() {
        let body_only = FIXTURE.split("  Symbol Table").next().unwrap();
        let t = SymbolTable::parse(body_only).expect("body-only listing parses");
        assert_eq!(t.source(), TableSource::BodyLines);
        assert_eq!(t.len(), 10);
        // Losing the section is itself a damage signal, even though every symbol parsed fine.
        assert!(!t.is_intact());
        // Both halves agree on every address — the emitter writes one sorted list twice.
        let full = table();
        for s in t.symbols() {
            assert_eq!(
                full.by_name(&s.name).map(|f| f.addr),
                Some(s.addr),
                "body/table address disagreement for {}",
                s.name
            );
        }
    }

    #[test]
    fn markers_are_read_from_the_type_and_unused_columns() {
        let t = table();
        assert_eq!(t.by_name("EntryPoint").unwrap().kind, SymbolKind::Code);
        assert_eq!(t.by_name("Character_ID").unwrap().kind, SymbolKind::Equate);
        assert!(t.by_name("__align$engine.boot_data$0").unwrap().unused);
        assert!(!t.by_name("EntryPoint").unwrap().unused);
    }

    /// Trap 1 + trap 2: `FFFF8CFA` is an 8-hex RAM address, and it means the bus address `$FF8CFA`.
    #[test]
    fn ram_addresses_are_plain_8_hex_and_mask_to_24_bits() {
        let t = table();
        let p = t.by_name("Player_1").unwrap();
        assert_eq!(p.raw_addr, 0xFFFF_8CFA, "raw listing spelling must survive");
        assert_eq!(p.addr, 0x00FF_8CFA, "lookup address must be 24-bit");
        assert_eq!(p.space(), AddrSpace::Ram);
        // Either spelling of the same address must find it.
        assert_eq!(t.address_of("Player_1"), Some(0x00FF_8CFA));
        assert_eq!(t.resolve(0xFFFF_8CFA).unwrap().symbol.name, "Player_1");
        assert_eq!(t.resolve(0x00FF_8CFA).unwrap().symbol.name, "Player_1");
    }

    /// Trap 3: the type column does not separate code from RAM — `Player_1` is a RAM variable marked `C`.
    #[test]
    fn type_column_does_not_discriminate_code_from_ram() {
        let t = table();
        let p = t.by_name("Player_1").unwrap();
        assert_eq!(p.kind, SymbolKind::Code);
        assert_eq!(p.space(), AddrSpace::Ram, "only the address range tells us");
        assert_eq!(t.by_name("EntryPoint").unwrap().space(), AddrSpace::Rom);
    }

    #[test]
    fn resolve_reports_nearest_preceding_and_displacement() {
        let t = table();
        let r = t.resolve(0x0000_021A).unwrap();
        assert_eq!(r.symbol.demangled, "EntryPoint.warm_boot");
        assert_eq!(r.displacement, 6);
        assert_eq!(r.to_string(), "EntryPoint.warm_boot+$6");
        // Exact hit prints bare.
        assert_eq!(t.resolve(0x200).unwrap().to_string(), "EntryPoint");
    }

    /// `name()` and `Display` are different products for different consumers, and conflating them is a
    /// bug that hides at every address that happens to land exactly on a label.
    ///
    /// `Display` is the disassembly form a human reads; `name()` is the **identifying** spelling that
    /// must round-trip. The Aether bus's `$defs/symbolName` rejects a `+$hex` suffix by pattern, so a
    /// wire field fed from `Display` is conformant only when the displacement is zero — which is exactly
    /// how the server shipped it until a test finally read at a displaced address.
    #[test]
    fn a_resolutions_name_is_the_round_tripping_spelling_never_the_display_form() {
        let t = table();
        for (addr, name) in [
            (0x0000_021Au32, "EntryPoint.warm_boot"),
            (0x200, "EntryPoint"),
        ] {
            let r = t.resolve(addr).unwrap();
            assert_eq!(
                r.name(),
                name,
                "no displacement suffix, at any displacement"
            );
            assert!(!r.name().contains("+$"));
            // The round trip the wire rule exists for: the name resolves back to the symbol it named.
            assert_eq!(t.address_of(r.name()), Some(r.symbol.addr));
        }
        // Display still carries the suffix — this is a split, not a rename.
        assert_eq!(
            t.resolve(0x0000_021A).unwrap().to_string(),
            "EntryPoint.warm_boot+$6"
        );
    }

    #[test]
    fn resolve_never_crosses_an_address_space() {
        let t = table();
        // Work RAM below the first RAM symbol must not resolve to the last ROM symbol (`EndOfRom`) with a
        // ~15 MB displacement — the classic confidently-wrong answer.
        assert!(t.resolve(0x00FF_0000).is_none());
        // An unmapped gap (VDP ports) resolves to nothing at all.
        assert!(t.resolve(0x00C0_0004).is_none());
        // Below every symbol.
        assert!(t.resolve(0x0000_0100).is_none());
    }

    #[test]
    fn resolve_within_rejects_an_implausibly_distant_answer() {
        let t = table();
        // $2A11F0 is 2 MB past EndOfRom ($A11F0) — off the end of a 696 KB cart, but still inside the
        // ROM decode window, so nearest-preceding happily hands back `EndOfRom` + $200000.
        let far = t.resolve(0x002A_11F0).expect("still ROM space");
        assert_eq!(far.symbol.name, "EndOfRom");
        assert_eq!(far.displacement, 0x0020_0000);
        assert!(t.resolve_within(0x002A_11F0, 0x100).is_none());
        assert!(t.resolve_within(0x0000_021A, 0x100).is_some());
    }

    #[test]
    fn aliased_addresses_are_all_reachable() {
        let t = table();
        let at = t.symbols_at(0x214);
        assert_eq!(at.len(), 2, "two labels share $214: {at:?}");
        assert!(at.iter().any(|s| s.demangled == "EntryPoint.wait_dma"));
        assert!(at.iter().any(|s| s.demangled == "EntryPoint.warm_boot"));
        assert!(t.symbols_at(0x216).is_empty());
    }

    #[test]
    fn scope_tree_splits_on_dollar() {
        let t = table();
        let s = t.by_name("$engine.boot$EntryPoint$wait_dma").unwrap();
        let sc = s.scope();
        assert_eq!(sc.module, Some("engine.boot"));
        assert_eq!(sc.parent, Some("EntryPoint"));
        assert_eq!(sc.local, "wait_dma");
        assert_eq!(sc.outer, None);
        assert_eq!(s.demangled, "EntryPoint.wait_dma");
        // A plain name has only a local level, and demangles to itself.
        let p = t.by_name("EntryPoint").unwrap();
        assert_eq!(p.scope().module, None);
        assert_eq!(p.scope().parent, None);
        assert_eq!(p.demangled, "EntryPoint");
        // The module index is the top of the tree.
        assert_eq!(
            t.modules(),
            vec!["engine.bg_anim", "engine.boot", "engine.boot_data"]
        );
        assert_eq!(t.symbols_in_module("engine.boot").len(), 3);
        assert!(t.symbols_in_module("nope").is_empty());
    }

    /// Regression for the shape a positional module rule gets wrong: a label emitted inside a macro carries
    /// the macro instance *outside* the module (`$diag2$engine.bg_anim$raise`). Reading the module as "the
    /// first component" invents a phantom module `diag2` — on the real `s4.debug.lst` that means 34 phantom
    /// modules and 125 misfiled symbols. `s4.lst` contains none of these, so only a debug build exposes it.
    #[test]
    fn macro_scoped_labels_find_the_module_by_the_dot_not_by_position() {
        let t = table();
        let s = t.by_name("$diag2$engine.bg_anim$raise").unwrap();
        let sc = s.scope();
        assert_eq!(
            sc.outer,
            Some("diag2"),
            "the macro instance is the outer scope"
        );
        assert_eq!(sc.module, Some("engine.bg_anim"), "NOT `diag2`");
        assert_eq!(sc.parent, None, "there is no proc level in this shape");
        assert_eq!(sc.local, "raise");
        // No phantom module is indexed, and the real one collects both macro instances.
        assert!(t.symbols_in_module("diag2").is_empty());
        assert_eq!(t.symbols_in_module("engine.bg_anim").len(), 2);
        assert!(!t.modules().iter().any(|m| m.starts_with("diag")));
        // The *display* name still follows sigil's own rule (last two components), so it reads the same as
        // the on-target MD Debugger shows it — module + label here, not parent + label.
        assert_eq!(s.demangled, "engine.bg_anim.raise");
        // `__align$engine.boot_data$0` is the same shape and must resolve the same way.
        let a = t.by_name("__align$engine.boot_data$0").unwrap();
        assert_eq!(a.scope().outer, Some("__align"));
        assert_eq!(a.scope().module, Some("engine.boot_data"));
    }

    /// Two macro expansions of the same label share a demangled spelling at *different* addresses, so that
    /// spelling does not identify a location. `Display` must fall back to the unique raw name rather than
    /// print a name that means two places (24 such collisions exist in the real `s4.debug.lst`).
    #[test]
    fn an_ambiguous_demangled_name_is_never_displayed() {
        let t = table();
        let a = t.by_name("$diag2$engine.bg_anim$raise").unwrap();
        let b = t.by_name("$diag3$engine.bg_anim$raise").unwrap();
        assert_eq!(a.demangled, b.demangled);
        assert_ne!(a.addr, b.addr);
        assert!(a.demangled_ambiguous && b.demangled_ambiguous);
        // The raw name is unique, so the printed answer stays exact.
        let r = t.resolve(0x2C4).unwrap();
        assert_eq!(r.to_string(), "$diag2$engine.bg_anim$raise+$4");
        // An unambiguous name is still printed in its readable form.
        assert!(!t.by_name("EntryPoint").unwrap().demangled_ambiguous);
        assert_eq!(t.resolve(0x200).unwrap().to_string(), "EntryPoint");
        // Aliases at the SAME address are not ambiguous — either name is a correct answer.
        assert!(
            !t.by_name("$engine.boot$EntryPoint$wait_dma")
                .unwrap()
                .demangled_ambiguous
        );
    }

    /// `is_intact` must catch the truncation shapes a bare footer-count comparison misses.
    #[test]
    fn is_intact_catches_every_damage_shape() {
        assert!(table().is_intact());
        // Footer present but the count disagrees (a row was deleted).
        let missing_row = FIXTURE.replace(" EndOfRom : A11F0 C |\n", "");
        let t = SymbolTable::parse(&missing_row).unwrap();
        assert_eq!(t.matches_declared_count(), Some(false));
        assert!(!t.is_intact());
        // Footer gone entirely (the usual truncation) — the count check returns `None`, not `Some(false)`,
        // which is exactly why `is_intact` cannot be built on it alone.
        let no_footer = FIXTURE.replace("   10 symbols\n", "");
        let t = SymbolTable::parse(&no_footer).unwrap();
        assert_eq!(t.matches_declared_count(), None);
        assert!(!t.is_intact());
        // An unrecognised row inside the section.
        let junk = FIXTURE.replace(" EndOfRom : A11F0 C |", " EndOfRom : ZZZZ C |");
        let t = SymbolTable::parse(&junk).unwrap();
        assert_eq!(t.skipped_lines(), 1);
        assert!(!t.is_intact());
    }

    /// The fail-open path the binding check must not have: a listing that *would* be refused becomes
    /// merely `Indeterminate` once its `EndOfRom` row is gone. `is_intact` is what lets a caller tell that
    /// apart from an honestly-unfingerprinted listing.
    #[test]
    fn a_mismatch_that_lost_its_fingerprint_is_still_detectable() {
        let no_end = FIXTURE.replace(" EndOfRom : A11F0 C |\n", "");
        let t = SymbolTable::parse(&no_end).unwrap();
        assert_eq!(
            t.validate_against_rom(&rom_with_appendix(0xA11F0, 35_860)),
            RomBinding::Indeterminate(Indeterminate::NoEndOfRomSymbol),
            "losing the fingerprint downgrades the verdict — that is the trap"
        );
        assert!(
            !t.is_intact(),
            "…but the file is visibly damaged, so a caller can still refuse"
        );
    }

    #[test]
    fn synthetic_plumbing_is_flagged_but_kept() {
        let t = table();
        assert!(
            t.by_name("__align$engine.boot_data$0")
                .unwrap()
                .is_synthetic
        );
        assert!(
            t.by_name("$engine.boot$asm1$wait_z80")
                .unwrap()
                .is_synthetic
        );
        assert!(
            !t.by_name("$engine.boot$EntryPoint$wait_dma")
                .unwrap()
                .is_synthetic
        );
        assert!(!t.by_name("EntryPoint").unwrap().is_synthetic);
        // `asm` and `asmFoo` are real names, not block scopes (mirrors sigil's own precision test).
        assert!(is_asm_block_scope("asm0"));
        assert!(is_asm_block_scope("asm12"));
        assert!(!is_asm_block_scope("asm"));
        assert!(!is_asm_block_scope("asmName"));
    }

    #[test]
    fn prefix_search_works_on_both_spellings() {
        let t = table();
        let raw = t.with_prefix("$engine.boot$EntryPoint$");
        assert_eq!(raw.len(), 2);
        let dem = t.with_demangled_prefix("EntryPoint.");
        assert_eq!(
            dem.len(),
            2,
            "{:?}",
            dem.iter().map(|s| &s.demangled).collect::<Vec<_>>()
        );
        // `EntryPoint` itself is a prefix of `EntryPoint.wait_dma`, so it matches all three.
        assert_eq!(t.with_demangled_prefix("EntryPoint").len(), 3);
        assert!(t.with_prefix("zzz").is_empty());
    }

    #[test]
    fn ambiguous_demangled_name_refuses_to_guess() {
        // Two modules, each with a `Foo.bar`. `address_of` must not pick one.
        let src = "\
  Symbol Table (* = unused):

 $a.mod$Foo$bar : 100 C |
 $b.mod$Foo$bar : 200 C |

   2 symbols
";
        let t = SymbolTable::parse(src).unwrap();
        assert_eq!(t.by_demangled("Foo.bar").len(), 2);
        assert_eq!(t.address_of("Foo.bar"), None);
        // The unambiguous raw name still resolves.
        assert_eq!(t.address_of("$a.mod$Foo$bar"), Some(0x100));
    }

    /// Damaged rows are counted, never fatal — and a row that is merely *address-less* is not damage.
    ///
    /// The split is decided by the type letter (see [`parse_table_entry`]): a `C` row promises an address,
    /// so `NOTHEX` there is corruption; a `-` row is an equate, so a quoted string there is the ordinary
    /// output AS emits for `ARCHITECTURE`/`DATE`/`TIME`. Both are accounted for, in different buckets,
    /// and the footer reconciliation counts the second — which is what the two parses below pin.
    #[test]
    fn malformed_rows_are_skipped_and_counted_not_fatal() {
        let src = "\
  Symbol Table (* = unused):

 Good : 100 C |
 Bad : NOTHEX C |
 AlsoBad : 200 X |
 Truncated : 300
 StringEquate : \"text\" - |

   1 symbols
";
        let t = SymbolTable::parse(src).unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t.by_name("Good").unwrap().addr, 0x100);
        // `Bad` (a C row with a non-hex value), `AlsoBad` (an unknown type letter) and `Truncated` (no
        // type column at all) are damage. `StringEquate` is not.
        assert_eq!(t.skipped_lines(), 3);
        assert_eq!(t.non_address_rows(), 1);
        assert_eq!(t.non_address_unused(), 0);
        // AS's footer counts its string pseudo-symbols, so a footer of 1 no longer reconciles against
        // 1 symbol + 1 address-less row…
        assert_eq!(t.matches_declared_count(), Some(false));
        // …and a footer of 2 does. Both directions, so the accounting cannot be vacuous.
        let two = SymbolTable::parse(&src.replace("   1 symbols", "   2 symbols")).unwrap();
        assert_eq!(two.matches_declared_count(), Some(true));
        assert!(
            !two.is_intact(),
            "the three damaged rows still make it not-intact"
        );
    }

    #[test]
    fn a_truncated_listing_is_detectable_via_the_footer() {
        let src = "\
  Symbol Table (* = unused):

 Good : 100 C |

   9 symbols
";
        let t = SymbolTable::parse(src).unwrap();
        assert_eq!(t.matches_declared_count(), Some(false));
    }

    #[test]
    fn a_non_listing_is_a_parse_error() {
        assert_eq!(
            SymbolTable::parse("hello\nworld\n").unwrap_err(),
            SymbolParseError::NoSymbols
        );
        assert_eq!(
            SymbolTable::parse("").unwrap_err(),
            SymbolParseError::NoSymbols
        );
    }

    /// Build a stand-in ROM image with a `deb2` appendix at `end`.
    fn rom_with_appendix(end: usize, appendix_len: usize) -> Vec<u8> {
        let mut rom = vec![0u8; end + appendix_len];
        rom[end] = DEB2_MAGIC[0];
        rom[end + 1] = DEB2_MAGIC[1];
        rom
    }

    #[test]
    fn shape_match_accepts_the_rom_the_listing_describes() {
        let t = table();
        let rom = rom_with_appendix(0xA11F0, 35_860);
        assert_eq!(
            t.validate_against_rom(&rom),
            RomBinding::Match {
                appendix_offset: 0xA11F0,
                appendix_len: 35_860
            }
        );
    }

    /// Trap 4, the load-bearing case: a debug-shape listing against a release ROM. The debug listing's
    /// `EndOfRom` ($A30B0) lands inside the release image's appendix, where there is no magic.
    #[test]
    fn shape_mismatch_is_refused_not_tolerated() {
        let debug_lst = FIXTURE.replace("A11F0", "A30B0");
        let t = SymbolTable::parse(&debug_lst).unwrap();
        let release_rom = rom_with_appendix(0xA11F0, 35_860);
        match t.validate_against_rom(&release_rom) {
            RomBinding::Mismatch(BindingFault::NoAppendixMagic { offset, found }) => {
                assert_eq!(offset, 0xA30B0);
                assert_eq!(found, [0, 0]);
            }
            other => panic!("shape cross must be refused, got {other:?}"),
        }
    }

    #[test]
    fn end_of_rom_past_the_image_is_a_mismatch() {
        let t = table();
        let short = rom_with_appendix(0x1000, 0x2000);
        match t.validate_against_rom(&short) {
            RomBinding::Mismatch(BindingFault::EndOfRomOutOfRange {
                end_of_rom,
                rom_len,
            }) => {
                assert_eq!(end_of_rom, 0xA11F0);
                assert_eq!(rom_len, 0x3000);
            }
            other => panic!("expected out-of-range, got {other:?}"),
        }
    }

    #[test]
    fn a_coincidental_magic_with_no_room_for_a_table_is_refused() {
        let t = table();
        let rom = rom_with_appendix(0xA11F0, 16);
        match t.validate_against_rom(&rom) {
            RomBinding::Mismatch(BindingFault::AppendixTooSmall { offset, len }) => {
                assert_eq!(offset, 0xA11F0);
                assert_eq!(len, 16);
            }
            other => panic!("expected too-small, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------------------------------
    // The stock-AS dialect (`s1disasm/sonic.lst`) — see the module docs.
    // -----------------------------------------------------------------------------------------------

    /// A miniature stock-AS `Symbol Table`, transcribed from real `sonic.lst` rows rather than invented:
    /// two symbols per line separated by `|`, 48-bit sign-extended RAM addresses, `v_player` equ-defined
    /// (type `-`) while `f_debugmode` is a label (type `C`), a genuine `-` constant with a tiny value, and
    /// AS's own starred string/float metadata rows. `EndOfRom` is the image length exactly, as
    /// `RomEndLoc: dc.l EndOfRom-1` makes it.
    const AS_FIXTURE: &str = "\
  Symbol Table (* = unused):
  --------------------------

 AddPLC :                      1578 C |  AddPLC.findspace :            1590 C |
*ARCHITECTURE :                                      \"x86_64-unknown-linux\" - |
*CONSTPI :        3.141592653589793 - |
*DATE :                \"08/19/2026\" - |
*TIME :               \"11:35:59 PM\" - |
 AniArt_MZ_Lava.size :            8 - |  ArtTile_Level :                  0 - |
 EndOfRom :                   86978 C |  RomEndLoc :                    1A4 C |
 f_debugmode :     FFFFFFFFFFFFFFFA C |  v_palette_line_2 : FFFFFFFFFFFFFB20 C |
 v_player :        FFFFFFFFFFFFD000 - |

   13 symbols
    4 unused symbols
";

    fn as_table() -> SymbolTable {
        SymbolTable::parse(AS_FIXTURE).expect("the AS fixture parses")
    }

    /// AS packs the table two columns wide. Splitting on `|` is what recovers the second column — without
    /// it every one of these lines is a ten-token row that matches nothing, which is how 4,174 of
    /// `sonic.lst`'s rows used to vanish. Negative control: the same content one-per-line must give the
    /// same table, so the splitter cannot be reading the second column *instead* of the first.
    #[test]
    fn as_packs_two_symbols_per_line_and_both_are_read() {
        let t = as_table();
        assert_eq!(t.address_of("AddPLC"), Some(0x1578));
        assert_eq!(t.address_of("AddPLC.findspace"), Some(0x1590));
        assert_eq!(t.address_of("EndOfRom"), Some(0x8_6978));
        assert_eq!(t.address_of("RomEndLoc"), Some(0x1A4));
        assert_eq!(t.address_of("f_debugmode"), Some(0xFF_FFFA));
        assert_eq!(t.address_of("v_palette_line_2"), Some(0xFF_FB20));

        let one_per_line = AS_FIXTURE.replace(" |  ", " |\n ");
        let split = SymbolTable::parse(&one_per_line).expect("parses one-per-line too");
        assert_eq!(split.len(), t.len(), "the two spellings must agree");
        for s in t.symbols() {
            assert_eq!(
                split.address_of(&s.name),
                Some(s.addr),
                "{} differs between the packed and unpacked spellings",
                s.name
            );
        }
    }

    /// The 48-bit sign-extended RAM spelling. Every address the classic loop needs is one of these, and
    /// each overflows `u32` — parsed as `u64` and masked, `FFFFFFFFFFFFD000` is the bus address `$FFD000`.
    /// The `addr == raw_addr & BUS_ADDR_MASK` invariant must survive the widening.
    #[test]
    fn as_sign_extended_ram_addresses_mask_to_the_bus_address() {
        let t = as_table();
        let p = t.by_name("v_player").expect("v_player is in the table");
        assert_eq!(
            p.addr, 0x00FF_D000,
            "the 24 lines the 68000 actually drives"
        );
        assert_eq!(p.raw_addr, 0xFFFF_D000, "the conventional 32-bit spelling");
        assert_eq!(p.addr, p.raw_addr & BUS_ADDR_MASK);
        assert_eq!(p.space(), AddrSpace::Ram);
        // The most-negative one in the file, where a truncation bug would be least visible.
        assert_eq!(t.address_of("f_debugmode"), Some(0x00FF_FFFA));
        // A caller may pass either spelling to the reverse direction.
        let r = t.resolve(0xFFFF_FFFA).expect("f_debugmode resolves");
        assert_eq!(r.symbol.name, "f_debugmode");
        assert_eq!(r.displacement, 0);
    }

    /// **The forward-only ruling.** An AS `-` row may be a RAM address (`v_player`) or a bare constant
    /// (`AniArt_MZ_Lava.size = 8`), and the row says nothing that tells them apart — so both resolve
    /// name→value and neither answers value→name.
    ///
    /// The constant is the reason for the rule: `$8` and `$0` sit below every real label, so admitting them
    /// to nearest-preceding search would hand a confident name to any low address. The RAM label is the
    /// reason it is forward-*only* rather than skip-wholesale: `v_player` is exactly the symbol the classic
    /// warp path pokes by name.
    #[test]
    fn as_equ_rows_resolve_forward_only() {
        let t = as_table();

        // Forward: both kinds resolve, to their masked value.
        assert_eq!(t.address_of("v_player"), Some(0x00FF_D000));
        assert_eq!(t.address_of("AniArt_MZ_Lava.size"), Some(0x8));
        assert_eq!(t.address_of("ArtTile_Level"), Some(0x0));
        assert!(t.by_name("v_player").is_some());
        assert_eq!(
            t.with_prefix("v_pl").len(),
            1,
            "prefix search sees them too"
        );
        for n in ["v_player", "AniArt_MZ_Lava.size", "ArtTile_Level"] {
            assert_eq!(
                t.by_name(n).unwrap().kind,
                SymbolKind::Equate,
                "{n} must be kind-tagged as equ-derived"
            );
            assert!(!t.by_name(n).unwrap().resolves_in_reverse());
        }

        // Reverse: none of them, ever — not as an exact hit and not as a nearest-preceding answer.
        for (name, value) in [
            ("v_player", 0x00FF_D000u32),
            ("AniArt_MZ_Lava.size", 0x8),
            ("ArtTile_Level", 0x0),
        ] {
            assert!(
                t.symbols_at(value).iter().all(|s| s.name != name),
                "{name} answered an exact addr->name query at ${value:X}"
            );
            if let Some(r) = t.resolve(value) {
                assert_ne!(
                    r.symbol.name, name,
                    "{name} answered nearest-preceding at ${value:X}"
                );
            }
        }
        // The constants are below every label, so those two addresses must name nothing at all — the
        // confidently-wrong answer the rule exists to prevent.
        assert!(t.resolve(0x0).is_none());
        assert!(t.resolve(0x8).is_none());
        // And the RAM address the equ row *does* describe still names nothing, rather than naming the
        // nearest code label across the address-space boundary.
        assert!(t.resolve(0x00FF_D000).is_none());
        // A real label in the same space is unaffected — the exclusion is per-row, not per-file.
        assert_eq!(
            t.resolve(0x00FF_FB20).map(|r| r.symbol.name.as_str()),
            Some("v_palette_line_2")
        );
    }

    /// AS's build metadata (`ARCHITECTURE`, `CONSTPI`, `DATE`, `TIME`) is well-formed output, not damage:
    /// consumed, counted separately, and reconciled against **both** of the file's own footers, which count
    /// these rows. Without that the listing reports as truncated and the frontend's load policy refuses it.
    #[test]
    fn as_metadata_rows_are_consumed_and_reconcile_both_footers() {
        let t = as_table();
        assert_eq!(t.skipped_lines(), 0, "AS metadata is not damage");
        assert_eq!(t.non_address_rows(), 4);
        assert_eq!(t.non_address_unused(), 4, "all four are `*`-marked");
        assert_eq!(t.len(), 9, "…and none of them became a symbol");
        for n in ["ARCHITECTURE", "CONSTPI", "DATE", "TIME"] {
            assert!(t.by_name(n).is_none(), "{n} leaked into the table");
            assert_eq!(t.address_of(n), None);
        }
        // Both footers close: 9 + 4 == 13 symbols, 0 + 4 == 4 unused.
        assert_eq!(t.matches_declared_count(), Some(true));
        assert_eq!(
            t.declared_unused(),
            Some(t.symbols().iter().filter(|s| s.unused).count() + t.non_address_unused())
        );
        assert!(t.is_intact(), "a healthy AS listing must read as whole");
    }

    /// **The binding ruling (2026-08-19).** `RomEndLoc: dc.l EndOfRom-1` puts `EndOfRom` at exactly the
    /// image length, so a stock AS listing has no appendix to probe. That exact equality is the
    /// no-appendix marker and downgrades to `Indeterminate` — accepted-unverified under the caller's
    /// existing policy, since `is_intact` is true.
    ///
    /// The three assertions after it are the guard this must not have weakened, and they are the point of
    /// the test: one byte either side of the equality is still a positive `Mismatch`.
    #[test]
    fn end_of_rom_at_exactly_the_image_length_is_a_no_appendix_marker() {
        let t = as_table();
        let len = 0x8_6978usize;
        assert_eq!(
            t.validate_against_rom(&vec![0u8; len]),
            RomBinding::Indeterminate(Indeterminate::EndOfRomIsImageEnd { rom_len: len }),
            "an appendix-less ROM is unverifiable, not wrong"
        );
        assert!(t.is_intact(), "so the caller accepts it with the caveat");

        // One byte shorter: `EndOfRom` is now past the end. Still a fault.
        assert!(matches!(
            t.validate_against_rom(&vec![0u8; len - 1]),
            RomBinding::Mismatch(BindingFault::EndOfRomOutOfRange { .. })
        ));
        // One byte longer: in range, but with no room for the two magic bytes. Still a fault.
        assert!(matches!(
            t.validate_against_rom(&vec![0u8; len + 1]),
            RomBinding::Mismatch(BindingFault::EndOfRomOutOfRange { .. })
        ));
        // Comfortably in range with the wrong bytes there — the 92.6% wrong-listing guard, untouched.
        assert!(matches!(
            t.validate_against_rom(&vec![0u8; len + 0x4000]),
            RomBinding::Mismatch(BindingFault::NoAppendixMagic { .. })
        ));
    }

    #[test]
    fn a_listing_without_end_of_rom_is_indeterminate_not_a_mismatch() {
        let src = "\
  Symbol Table (* = unused):

 Main : 100 C |

   1 symbols
";
        let t = SymbolTable::parse(src).unwrap();
        assert_eq!(
            t.validate_against_rom(&rom_with_appendix(0xA11F0, 35_860)),
            RomBinding::Indeterminate(Indeterminate::NoEndOfRomSymbol)
        );
    }

    #[test]
    fn rom_declared_end_reads_the_header_longword() {
        let mut rom = vec![0u8; 0x200];
        rom[0x1A4..0x1A8].copy_from_slice(&0x000A_A203u32.to_be_bytes());
        assert_eq!(rom_declared_end(&rom), Some(0x000A_A203));
        assert_eq!(rom_declared_end(&[0u8; 4]), None);
    }
}
