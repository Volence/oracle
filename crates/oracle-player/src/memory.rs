//! **The Memory panel** — one hex view over five address spaces, with a space selector rather than five
//! tabs (design §2.1: five tabs would be five scroll positions to keep in your head).
//!
//! # The two routes, and where the line falls (design §4.4)
//!
//! * **Reads go direct.** [`view`] calls `oracle_aether::engine`'s free `debug_read` / `vdp_space_read` /
//!   `z80_read_window` — *the same functions the five read handlers call*, made free and public by this
//!   parcel for exactly this reason. The panel does not round-trip a repaint through JSON, and it does not
//!   own a second region decode, a second bound check or a second Z80 mirror fold to drift from theirs.
//! * **Every gesture goes through [`Bus::call`].** The address box's symbol acceptance, the write, and
//!   the hash button are one-per-human-action, so each is answered by `Engine::dispatch` and the panel
//!   renders **the tool's own reply and the tool's own refusal, verbatim**. It composes no error text of
//!   its own about a server it is living inside.
//!
//! # ⚑ The paused-write asymmetry is REFLECTED, not fixed
//!
//! `write_memory`, `write_cram` and `z80_write` refuse a running machine with `-32005 machineRunning`.
//! **`write_vram` does not** — engine.rs's handler carries no `require_paused`, and that is a documented
//! decision rather than an oversight: the row's contract fragment does not name it in §6's run-control
//! rule, and *"relaxing a refusal later is additive (D5); introducing one is not"*, so the server serves
//! the gate it was given and files the argument for one upstream (audit D-16, deviation 1).
//!
//! So a human here can poke VRAM mid-frame and is refused the identical gesture on work RAM. The panel
//! **shows that**, and gating VRAM for tidiness would be the same lie in the opposite direction: a panel
//! that refuses what the tool allows misdescribes the server just as surely as one that allows what the
//! tool refuses. The owner's standing rule is that a panel shows the same answer a tool gets, and the
//! asymmetry is a property of the answer.
//!
//! # How the panel knows, without a table it wrote down
//!
//! [`probe_write_gate`] asks the handler. It dispatches the write method with an **empty params object**
//! and reads the code back: `require_paused` is the first statement in all three gated handlers, so a
//! machine-running refusal arrives *before* any param is parsed and before any byte could land, while an
//! open gate falls through to the handler's own `-32602` about the missing payload. Nothing is written on
//! either path.
//!
//! That is derived rather than copied — no list of "these three are gated" exists in this crate — but it
//! is derived from a **check order**, and a check order can be edited. So it is not trusted on its own:
//! `the_panels_write_gate_agrees_with_what_a_real_write_actually_does` drives a real, well-formed write
//! into every space in both run states and asserts the gate predicted the outcome. If someone ever moves
//! `require_paused` below the param parsing, the probe goes quiet and that test goes red.

use oracle_aether::engine::{self, METHODS};
use oracle_aether::rpc::{code, RpcError};
use oracle_core::system::System;
use oracle_core::watchpoints::WatchSpace;
use serde_json::{json, Value};

use crate::bus::{Answer, Bus};

// ---------------------------------------------------------------------------------------------------
// The five spaces
// ---------------------------------------------------------------------------------------------------

/// One of the five address spaces the Memory panel shows.
///
/// **`bus` and `rom+ram` are one space here, and the design doc's five-way list is wrong about that.**
/// `emulator/read {space:"bus"}` and `emulator/read_memory` both resolve through the *same*
/// `debug_read`, over the same ROM and work-RAM windows, returning the same bytes and the same region
/// label. Two selector entries for one derivation would be the believable wrong answer parcel 2a
/// refused for `A7`/`SP` — a reader takes two rows for two things. The fifth space is `vsram` instead,
/// which `emulator/read` serves and the doc's list dropped.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Space {
    /// The 68000 bus: cartridge ROM and the work-RAM window. The only space with a region label and the
    /// only one a symbol can name.
    Bus,
    Vram,
    Cram,
    Vsram,
    Z80,
}

impl Space {
    pub const ALL: [Space; 5] = [
        Space::Bus,
        Space::Vram,
        Space::Cram,
        Space::Vsram,
        Space::Z80,
    ];

    /// The selector's label. The parenthetical on `bus` is what stops a reader hunting for a separate
    /// "rom+ram" entry that would show the identical bytes.
    pub fn label(self) -> &'static str {
        match self {
            Space::Bus => "bus (ROM + work RAM)",
            Space::Vram => "vram",
            Space::Cram => "cram",
            Space::Vsram => "vsram",
            Space::Z80 => "z80",
        }
    }

    /// The served method whose answer this space's hex view reproduces. Named on the panel so a human can
    /// ask the same question from a tool and know which row to reach for.
    pub fn read_method(self) -> &'static str {
        match self {
            // `emulator/read_memory` is the same bytes through the same `debug_read`; `read` is named
            // because it is the row that takes a `space`, which is what this selector is.
            Space::Bus => "emulator/read",
            Space::Vram => "emulator/read_vram",
            Space::Cram => "emulator/read",
            Space::Vsram => "emulator/read",
            Space::Z80 => "emulator/z80_read",
        }
    }

    /// The served method a write in this space would go through, or `None` where the surface has none.
    ///
    /// **`Vsram` is `None` and that is the served surface, not a gap in this panel**: there is no
    /// `emulator/write_vsram` row. Checked, not assumed — [`write_gate`] looks the name up in [`METHODS`]
    /// rather than trusting this table, so a row added upstream turns the cell on by itself.
    pub fn write_method(self) -> Option<&'static str> {
        match self {
            Space::Bus => Some("emulator/write_memory"),
            Space::Vram => Some("emulator/write_vram"),
            Space::Cram => Some("emulator/write_cram"),
            Space::Vsram => None,
            Space::Z80 => Some("emulator/z80_write"),
        }
    }

    /// The space's size **read off the machine**, or `None` for the bus — which has no single end, only
    /// two windows with a hole between them, and whose edges the read itself refuses at.
    pub fn len(self, sys: &System) -> Option<usize> {
        match self {
            Space::Bus => None,
            Space::Vram => Some(sys.vram().len()),
            Space::Cram => Some(sys.vdp().cram().len()),
            Space::Vsram => Some(sys.vdp().vsram().len()),
            // The Z80's *window*, which is twice its RAM: `$2000-$3FFF` mirrors `$0000-$1FFF` and the
            // mirror is the machine (see `z80_read_window`). Showing only 8 KB would hide half the
            // addresses `emulator/z80_read` accepts.
            Space::Z80 => Some(0x4000),
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// Reads — route (a), through the handlers' own functions
// ---------------------------------------------------------------------------------------------------

/// `len` bytes of `space` at `addr`, plus the region label on the one space that has one.
///
/// Every branch is a call into `oracle_aether::engine`'s free functions — the identical code
/// `emulator/read`, `emulator/read_memory`, `emulator/read_vram` and `emulator/z80_read` run. The
/// refusals are theirs too, which is why this returns [`RpcError`] rather than a `String`: the panel
/// shows the message a tool would have been given, down to the wording.
pub fn read(
    space: Space,
    sys: &System,
    addr: u32,
    len: usize,
) -> Result<(Vec<u8>, Option<&'static str>), RpcError> {
    match space {
        Space::Bus => engine::debug_read(sys, addr, len).map(|(b, r)| (b, Some(r))),
        Space::Vram => {
            engine::vdp_space_read(sys, WatchSpace::Vram, addr, len as u64).map(|b| (b, None))
        }
        Space::Cram => {
            engine::vdp_space_read(sys, WatchSpace::Cram, addr, len as u64).map(|b| (b, None))
        }
        Space::Vsram => {
            engine::vdp_space_read(sys, WatchSpace::Vsram, addr, len as u64).map(|b| (b, None))
        }
        Space::Z80 => engine::z80_read_window(sys, addr, len).map(|b| (b, None)),
    }
}

/// One line of the hex view.
pub struct HexRow {
    pub addr: u32,
    pub bytes: Vec<u8>,
}

impl HexRow {
    /// `"00 1A FF …"`, and the ASCII gutter beside it.
    pub fn hex(&self) -> String {
        self.bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn ascii(&self) -> String {
        self.bytes
            .iter()
            .map(|&b| {
                if (0x20..0x7F).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            })
            .collect()
    }
}

/// What the panel draws for one repaint: either a page of rows, or the read's own refusal.
pub struct View {
    pub base: u32,
    pub region: Option<&'static str>,
    pub rows: Vec<HexRow>,
    /// The refusal, verbatim, when the requested page does not exist. Never rendered as an empty grid: a
    /// blank hex view and a refused read look identical, and only one of them means "there is nothing
    /// here".
    pub error: Option<RpcError>,
    /// Set when the page was **shortened because the space ends**, so the view says so rather than
    /// silently showing fewer rows than were asked for. A short page that does not announce itself is the
    /// clipped-read failure in a different costume.
    pub truncated_to: Option<usize>,
}

/// A page of `rows × per_row` bytes at `base`, clipped **loudly** to the end of a sized space.
pub fn view(space: Space, sys: &System, base: u32, rows: usize, per_row: usize) -> View {
    let want = rows * per_row;
    let (len, truncated_to) = match space.len(sys) {
        Some(size) => {
            let left = size.saturating_sub(base as usize);
            if left < want {
                (left, Some(left))
            } else {
                (want, None)
            }
        }
        // The bus has no single end; the read refuses at the region edge and that refusal is the answer.
        None => (want, None),
    };
    if len == 0 {
        return View {
            base,
            region: None,
            rows: Vec::new(),
            error: Some(RpcError::new(
                code::ADDRESS_OUT_OF_RANGE,
                format!(
                    "{} is at or past the end of {} ({} bytes)",
                    oracle_aether::hex::addr(base),
                    space.label(),
                    space.len(sys).unwrap_or(0)
                ),
            )),
            truncated_to: None,
        };
    }
    match read(space, sys, base, len) {
        Ok((bytes, region)) => View {
            base,
            region,
            rows: bytes
                .chunks(per_row)
                .enumerate()
                .map(|(i, c)| HexRow {
                    addr: base.wrapping_add((i * per_row) as u32),
                    bytes: c.to_vec(),
                })
                .collect(),
            error: None,
            truncated_to,
        },
        Err(e) => View {
            base,
            region: None,
            rows: Vec::new(),
            error: Some(e),
            truncated_to: None,
        },
    }
}

// ---------------------------------------------------------------------------------------------------
// The write gate — asked of the handler, never written down
// ---------------------------------------------------------------------------------------------------

/// Why a write cell is enabled or disabled, **in the words a tool would have been given**.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Gate {
    /// The handler would take this gesture right now.
    Open { method: &'static str },
    /// The handler refused for the machine's state. `reason` is the `-32005` discriminant
    /// (`machineRunning`); `message` is the handler's own sentence, shown to the human unchanged.
    Refused {
        method: &'static str,
        code: i64,
        reason: String,
        message: String,
    },
    /// The panel names a method the served surface does not carry.
    Unserved { method: &'static str },
    /// No method writes this space at all. Distinct from [`Unserved`](Gate::Unserved) because the two are
    /// different facts: one is a name that is wrong, the other is a capability that does not exist.
    NoMethod,
}

impl Gate {
    pub fn is_open(&self) -> bool {
        matches!(self, Gate::Open { .. })
    }

    /// The sentence the disabled cell shows. **Never blank and never a bare greyed box** — a control that
    /// does nothing and says nothing is indistinguishable from one that is broken.
    pub fn why(&self) -> String {
        match self {
            Gate::Open { method } => format!("writes go to {method}"),
            Gate::Refused {
                method,
                code,
                reason,
                message,
            } => format!("{method} refuses right now — {code} {reason}: {message}"),
            Gate::Unserved { method } => format!(
                "this build serves no method named {method}, so the panel has nothing to write through"
            ),
            Gate::NoMethod => "no served method writes this space — the bus has read rows for vsram \
                               (emulator/read) and no write row at all, so this is the surface's own \
                               limit and not a missing control"
                .into(),
        }
    }
}

/// Whether the surface carries `name` at all — from the dispatch table itself, which `initialize` also
/// builds its advertised `methods` array from, so the two cannot disagree.
pub fn is_served(name: &str) -> bool {
    METHODS.iter().any(|m| m.name == name)
}

/// **Ask the handler whether it would take a write right now**, without writing anything.
///
/// Dispatches the space's write method with `{}`. In all three gated handlers `require_paused` is the
/// first statement, so:
///
/// * a running machine answers `-32005 machineRunning` before a single param is looked at;
/// * a paused machine (and `write_vram` in either state) falls through to that handler's own `-32602`
///   about the payload it was not given.
///
/// Neither path reaches a `write8`, a `poke_vram`, a `poke_cram` or a `z80_ram_mut`. The probe is a
/// question, and the answer is the handler's.
pub fn probe_write_gate(bus: &mut Bus, sys: &mut System, space: Space) -> Gate {
    let Some(method) = space.write_method() else {
        return Gate::NoMethod;
    };
    if !is_served(method) {
        return Gate::Unserved { method };
    }
    match bus.call(sys, method, &json!({})) {
        Answer::Err(e) if e.code == code::INVALID_STATE => Gate::Refused {
            method,
            code: e.code,
            reason: e
                .data
                .as_ref()
                .and_then(|d| d.get("reason"))
                .and_then(Value::as_str)
                .unwrap_or("(the refusal carried no reason discriminant)")
                .to_string(),
            message: e.message,
        },
        // Any other outcome — a params refusal, or (impossible today) a success — means the state gate
        // did not fire, which is exactly what "open" means.
        _ => Gate::Open { method },
    }
}

/// The five gates, in [`Space::ALL`] order.
pub fn probe_all_gates(bus: &mut Bus, sys: &mut System) -> [Gate; 5] {
    [
        probe_write_gate(bus, sys, Space::Bus),
        probe_write_gate(bus, sys, Space::Vram),
        probe_write_gate(bus, sys, Space::Cram),
        probe_write_gate(bus, sys, Space::Vsram),
        probe_write_gate(bus, sys, Space::Z80),
    ]
}

// ---------------------------------------------------------------------------------------------------
// Writes — the params each row actually takes
// ---------------------------------------------------------------------------------------------------

/// Build the params for a write of `payload` (a hex byte string) at `addr` in `space`.
///
/// **CRAM is not addressed in bytes and the panel does not pretend otherwise.** `emulator/write_cram`
/// takes `line` / `index` / `raw`, because a palette entry is a 9-bit word and a byte-wide poke into one
/// is not a thing the chip has. So a CRAM write cell is a *word* cell: the byte address selects the
/// entry (`line = addr / 32`, `index = (addr % 32) / 2` — the inverse of the `cramAddr = entry × 2` the
/// read side publishes as its join key) and the payload is that entry's raw word. An odd address is
/// refused here rather than silently rounded down, which would write a neighbouring colour and report
/// success.
///
/// Errors are the panel's own only where the *panel's* input is malformed; everything the server can
/// judge is left to the server.
pub fn write_params(space: Space, addr: u32, payload: &str) -> Result<Value, String> {
    let clean = payload.trim().trim_start_matches("0x").replace(' ', "");
    if clean.is_empty() {
        return Err("nothing to write — type hex bytes".into());
    }
    if !clean.len().is_multiple_of(2) || !clean.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "{payload:?} is not a whole number of hex bytes (two digits each)"
        ));
    }
    match space {
        // The `0x` is not decoration: D9 category 1 makes it required, and `hex::parse_bytes` refuses a
        // bare `"AA"` with *"`bytes` must start with \"0x\" or \"$\""*. A panel that sent the digits
        // alone would show a human a `-32602` about their own perfectly good input.
        Space::Bus | Space::Vram | Space::Z80 => Ok(json!({
            "addr": oracle_aether::hex::addr(addr),
            "bytes": format!("0x{}", clean.to_uppercase()),
        })),
        Space::Cram => {
            if clean.len() != 4 {
                return Err(format!(
                    "a CRAM entry is one 9-bit word: give exactly two bytes (four hex digits), not {}",
                    clean.len() / 2
                ));
            }
            if !addr.is_multiple_of(2) {
                return Err(format!(
                    "{} is an odd byte address and a CRAM entry is two bytes wide — refused rather \
                     than rounded down onto the neighbouring colour",
                    oracle_aether::hex::addr(addr)
                ));
            }
            let raw = u16::from_str_radix(&clean, 16).map_err(|e| e.to_string())?;
            // **All three are JSON numbers, not hex strings.** D9 splits the vocabulary: an *address or
            // payload* is a `"0x…"` string (category 1), a *count, index or bounded value* is a number
            // (category 2), and `line`/`index`/`raw` are all category 2 — `parse_cram_line` refuses a
            // string outright and `raw` answers *"must be a non-negative integer (D9 category 2)"*. The
            // cell above sends `bytes` as a string in the same breath, which is why this is worth
            // stating rather than looking like an inconsistency.
            //
            // `raw` is also refused, never masked, for bits outside the chip's `0x0EEE` — the panel does
            // not pre-mask it, because a colour silently changed on the way to the chip is exactly the
            // wrong answer that refusal exists to make impossible.
            Ok(json!({
                "line": addr / 32,
                "index": (addr % 32) / 2,
                "raw": raw,
            }))
        }
        Space::Vsram => Err(Gate::NoMethod.why()),
    }
}

// ---------------------------------------------------------------------------------------------------
// The address box — which *is* `emulator/lookup_symbol` (design §2.2)
// ---------------------------------------------------------------------------------------------------

/// What the address box did with what was typed.
pub enum Resolved {
    /// A hex literal, parsed here. No dispatch: `"0x1234"` is not a question the server can answer better.
    Hex(u32),
    /// A name the server resolved. Carries the server's own reply so the panel can show the symbol it
    /// actually landed on, its displacement, and any `caveat` the server attached.
    Symbol { addr: u32, reply: Value },
    /// The server's refusal, verbatim.
    Refused(RpcError),
    /// The panel's own refusal, for input the server never sees.
    Rejected(String),
}

/// Resolve what a human typed into the address box, **through the served surface**.
///
/// * A hex literal is taken as one.
/// * Anything else is a name, and the name goes to the server:
///   * in the **bus** space, to `emulator/lookup_symbol`, which is what the design means by *"the address
///     box **is** this method"*;
///   * in every other space, to `emulator/read {space, symbol}` — which refuses, and refuses **in the
///     server's own words**: *"`symbol` is valid only with space \"bus\" — a VDP-internal byte address
///     has no symbol"*. That refusal is the answer, and asking for it costs one non-mutating read rather
///     than a second sentence this panel would have had to write and keep in step.
pub fn resolve_address(bus: &mut Bus, sys: &mut System, space: Space, text: &str) -> Resolved {
    let t = text.trim();
    if t.is_empty() {
        return Resolved::Rejected("type an address or a symbol name".into());
    }
    let hex = t
        .strip_prefix("0x")
        .or_else(|| t.strip_prefix('$'))
        .unwrap_or(t);
    if !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return match u32::from_str_radix(hex, 16) {
            Ok(a) => Resolved::Hex(a),
            Err(e) => Resolved::Rejected(format!("{t:?}: {e}")),
        };
    }
    if space != Space::Bus {
        // Deliberately a real call. See the doc above.
        return match bus.call(
            sys,
            "emulator/read",
            &json!({"space": space_wire(space), "symbol": t, "len": 1}),
        ) {
            Answer::Err(e) => Resolved::Refused(e),
            // Unreachable today (the handler refuses a non-bus symbol outright), and if it ever stops
            // being unreachable the honest thing is to use the answer rather than to assert about it.
            Answer::Ok(v) => match v.get("addr").and_then(Value::as_str) {
                Some(s) => match u32::from_str_radix(s.trim_start_matches("0x"), 16) {
                    Ok(a) => Resolved::Symbol { addr: a, reply: v },
                    Err(e) => Resolved::Rejected(format!("{s:?}: {e}")),
                },
                None => Resolved::Rejected(format!("the reply carried no `addr`: {v}")),
            },
        };
    }
    match bus.call(sys, "emulator/lookup_symbol", &json!({"name": t})) {
        Answer::Err(e) => Resolved::Refused(e),
        Answer::Ok(v) => match v.get("addr").and_then(Value::as_str) {
            Some(s) => match u32::from_str_radix(s.trim_start_matches("0x"), 16) {
                Ok(a) => Resolved::Symbol { addr: a, reply: v },
                Err(e) => Resolved::Rejected(format!("{s:?}: {e}")),
            },
            // A prefix search answers `matches`, not `addr` — a real reply shape, not an error, and the
            // panel says which one it got instead of showing nothing.
            None => Resolved::Rejected(format!(
                "{t:?} is not an exact name; the server answered a search instead: {v}"
            )),
        },
    }
}

/// The wire spelling of a space for `emulator/read`'s `space` param. `Z80` has none — that space is
/// `emulator/z80_read`'s own row — and callers must not reach here with it.
fn space_wire(space: Space) -> &'static str {
    match space {
        Space::Bus => "bus",
        Space::Vram => "vram",
        Space::Cram => "cram",
        Space::Vsram => "vsram",
        // `emulator/read` has no z80 space; the box falls back to naming the row that does.
        Space::Z80 => "z80",
    }
}

// ---------------------------------------------------------------------------------------------------
// memory_hash — a read you invoke, for a range you chose (design §2.2)
// ---------------------------------------------------------------------------------------------------

/// Whether `emulator/memory_hash` can hash `space`, **derived from the method's own closed params set**.
///
/// The row declares no `space` key, so it has no way to name anything but the 68000 bus — its target
/// resolves through the same `debug_read` the `bus` selector shows. That is a fact about the served
/// surface, read out of [`MethodSpec::params`], not a rule this panel decided.
pub fn hash_gate(space: Space) -> Result<(), String> {
    let name = "emulator/memory_hash";
    let Some(spec) = METHODS.iter().find(|m| m.name == name) else {
        return Err(format!("this build serves no {name}"));
    };
    if space == Space::Bus {
        return Ok(());
    }
    if spec.params.contains(&"space") {
        // A future revision that adds the key: the button turns on by itself rather than staying dark
        // behind a sentence somebody forgot to delete.
        return Ok(());
    }
    Err(format!(
        "{name} declares no `space` param (its keys are {:?}), so it hashes the 68000 bus only — \
         switch the selector to `bus` to hash a range",
        spec.params
    ))
}

/// `emulator/memory_hash` over `addr..addr+len`, answered by the handler.
pub fn hash(bus: &mut Bus, sys: &mut System, addr: u32, len: u64) -> Answer {
    bus.call(
        sys,
        "emulator/memory_hash",
        &json!({"addr": oracle_aether::hex::addr(addr), "len": len}),
    )
}

// ---------------------------------------------------------------------------------------------------
// The panel's own state
// ---------------------------------------------------------------------------------------------------

/// Bytes per hex row, and rows per page. Fixed rather than configurable in this parcel: a 16-wide row is
/// what every other hex view in the suite uses, and a knob nobody asked for is a knob to keep working.
pub const PER_ROW: usize = 16;
pub const ROWS: usize = 16;

/// Everything the Memory panel remembers between repaints.
pub struct MemoryPanel {
    pub space: Space,
    /// What is in the address box, verbatim — hex or a symbol name.
    pub addr_text: String,
    /// The resolved base of the page currently shown.
    pub base: u32,
    /// What the last address-box gesture answered, kept on screen until the next one.
    pub addr_note: Option<Line>,
    /// The write cell's payload, and the last write's answer.
    pub write_text: String,
    pub write_note: Option<Line>,
    /// The hash range's length box, and the last hash's answer.
    pub hash_len_text: String,
    pub hash_note: Option<Line>,
    /// The five write gates, and the pause state they were probed under. Re-probed **only** when that
    /// state changes: a gate is a question about the machine's mode, and asking it 60 times a second
    /// would be dispatching inside a repaint for an answer that cannot have moved.
    gates: [Gate; 5],
    gates_at_paused: Option<bool>,
}

impl Default for MemoryPanel {
    fn default() -> Self {
        Self {
            space: Space::Bus,
            addr_text: "0xFFFF0000".into(),
            base: 0xFFFF_0000,
            addr_note: None,
            write_text: String::new(),
            write_note: None,
            hash_len_text: "256".into(),
            hash_note: None,
            // Probed before the panel is ever drawn (see `gates_for`), so this initial value is never
            // shown. `NoMethod` rather than `Open`, because a placeholder that reads as permission is
            // the one wrong default here.
            gates: [
                Gate::NoMethod,
                Gate::NoMethod,
                Gate::NoMethod,
                Gate::NoMethod,
                Gate::NoMethod,
            ],
            gates_at_paused: None,
        }
    }
}

impl MemoryPanel {
    /// The gate for the selected space, re-probing if the machine's run state has moved since the last
    /// probe.
    pub fn gates_for(&mut self, bus: &mut Bus, sys: &mut System) -> &Gate {
        let paused = bus.is_paused();
        if self.gates_at_paused != Some(paused) {
            self.gates = probe_all_gates(bus, sys);
            self.gates_at_paused = Some(paused);
        }
        let i = Space::ALL
            .iter()
            .position(|s| *s == self.space)
            .unwrap_or(0);
        &self.gates[i]
    }

    /// The gate the panel is currently showing, without probing. For tests and for the report line.
    pub fn gate_of(&self, space: Space) -> &Gate {
        let i = Space::ALL.iter().position(|s| *s == space).unwrap_or(0);
        &self.gates[i]
    }
}

/// One line of panel text, and **whether it is a refusal**.
///
/// The flag travels beside the text rather than being read back out of it: the panel colours refusals,
/// and a renderer that decided by looking for a `"REFUSED"` prefix would be a second encoding of a fact
/// the [`Answer`] already carries — the kind that agrees until someone rewords the string.
///
/// The text itself is the reply's own JSON, or the refusal's own code, reason and message. Nothing here
/// paraphrases the server.
pub struct Line {
    pub text: String,
    pub refused: bool,
}

pub fn answer_line(a: &Answer) -> Line {
    Line {
        refused: a.is_err(),
        text: match a {
            Answer::Ok(v) => format!("ok — {v}"),
            Answer::Err(e) => match a.reason() {
                Some(r) => format!("REFUSED {} {r}: {}", e.code, e.message),
                None => format!("REFUSED {}: {}", e.code, e.message),
            },
        },
    }
}

impl Line {
    /// A line the **panel** produced about its own input, which never reached the server. Marked as a
    /// refusal so it is coloured like one — it is one — but worded so a reader can tell the two apart:
    /// "the panel cannot send that" is a different fact from "the server said no".
    pub fn from_panel(why: String) -> Self {
        Line {
            text: format!("the panel cannot send that: {why}"),
            refused: true,
        }
    }

    pub fn plain(text: String) -> Self {
        Line {
            text,
            refused: false,
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// The parity invariants — design §4.4 R3
// ---------------------------------------------------------------------------------------------------

/// **This panel and the five read rows must never disagree**, and neither must its write gate and the
/// four write rows.
///
/// The guard lives here rather than in `oracle-aether/tests/` for the structural reason `pick.rs` gives:
/// `oracle-player` is the crate that can see both sides. Every bus side goes through
/// [`crate::bus::Bus`] — the same `Host::call` the shipped panel uses — so a `call` that stopped
/// swapping the machine in would fail these too rather than quietly comparing two placeholders.
#[cfg(test)]
mod bus_parity {
    use super::*;
    use crate::bus::Answer;
    use oracle_aether::host::MachineInfo;
    use oracle_core::system::System;

    fn booted() -> System {
        let mut sys = System::new(0x5EED);
        sys.load_rom(oracle_core::testrom::build());
        sys.reset();
        sys.run_frames(7);
        sys
    }

    fn bus(sys: &mut System, paused: bool) -> Bus {
        Bus::new(sys, MachineInfo::default(), paused)
    }

    /// `"0x00AA55"` → the bytes. The bus spells payloads as hex strings (D9 category 1) and the panel
    /// carries them as bytes, so the comparison crosses that boundary explicitly rather than by matching
    /// two strings and calling it agreement.
    fn bytes_of(v: &Value) -> Vec<u8> {
        let s = v
            .get("bytes")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("the reply carries a `bytes` hex string, got {v}"));
        let d = s.trim_start_matches("0x");
        assert!(d.len().is_multiple_of(2), "an even number of hex digits: {s:?}");
        (0..d.len() / 2)
            .map(|i| u8::from_str_radix(&d[i * 2..i * 2 + 2], 16).expect("hex"))
            .collect()
    }

    fn ok(a: Answer) -> Value {
        match a {
            Answer::Ok(v) => v,
            Answer::Err(e) => panic!("expected an answer, got {} {}", e.code, e.message),
        }
    }

    /// **The read half: every space, the panel's bytes against the bus's own reply.**
    ///
    /// The bus side deliberately uses a *different method per space* where the surface has one
    /// (`emulator/read`, `emulator/read_vram`, `emulator/z80_read`), because that is what a human would
    /// reach for from a tool, and it is the pair that must agree. The panel side calls [`read`], which is
    /// what the shipped hex view calls.
    #[test]
    fn the_memory_panel_shows_the_same_bytes_the_bus_serves() {
        let mut sys = booted();
        let mut b = bus(&mut sys, false);
        const LEN: usize = 32;

        for (space, method, params) in [
            (
                Space::Bus,
                "emulator/read",
                json!({"space": "bus", "addr": "0x00FF0000", "len": LEN}),
            ),
            (
                Space::Vram,
                "emulator/read_vram",
                json!({"addr": "0x00000100", "len": LEN}),
            ),
            (
                Space::Cram,
                "emulator/read",
                json!({"space": "cram", "addr": "0x00000010", "len": LEN}),
            ),
            (
                Space::Vsram,
                "emulator/read",
                json!({"space": "vsram", "addr": "0x00000000", "len": LEN}),
            ),
            (
                Space::Z80,
                "emulator/z80_read",
                json!({"addr": "0x00000100", "len": LEN}),
            ),
        ] {
            let addr = u32::from_str_radix(
                params["addr"].as_str().unwrap().trim_start_matches("0x"),
                16,
            )
            .unwrap();
            let served = bytes_of(&ok(b.call(&mut sys, method, &params)));
            let (panel, _) = read(space, &sys, addr, LEN).expect("the panel reads this range");
            assert_eq!(
                panel,
                served,
                "{}: the panel and {method} have DRIFTED",
                space.label()
            );
            assert_eq!(served.len(), LEN, "{method} answered a short read");
        }
    }

    /// The `bus` space's **region label** is the handler's, not a second guess: work RAM and cartridge
    /// ROM are two different windows and mislabelling one would misdescribe what a poke is about to hit.
    #[test]
    fn the_bus_space_reports_the_same_region_the_bus_does() {
        let mut sys = booted();
        let mut b = bus(&mut sys, false);
        for (addr, expect) in [(0x00FF_0000u32, "work RAM"), (0x0000_0100, "cartridge ROM")] {
            let reply = ok(b.call(
                &mut sys,
                "emulator/read",
                &json!({"space": "bus", "addr": oracle_aether::hex::addr(addr), "len": 4}),
            ));
            let (_, region) = read(Space::Bus, &sys, addr, 4).expect("readable");
            assert_eq!(region, Some(expect), "the fixture moved");
            assert_eq!(
                region.map(str::to_string),
                reply["region"].as_str().map(str::to_string),
                "the panel's region label and the bus's have DRIFTED"
            );
        }
    }

    /// ⚑ **The write gate against reality — the asymmetry, measured rather than asserted.**
    ///
    /// For every space, in both run states: the gate says whether a write would be taken, then a real,
    /// well-formed write is made and the outcome is compared. Both directions fail loudly, so a gate
    /// that is wrong in either is caught.
    ///
    /// This is what stops [`probe_write_gate`] from being trusted on its own. The probe reads a *check
    /// order* — `require_paused` first — and a check order is editable. If someone moves it below the
    /// param parsing, the probe starts reporting Open for a handler that still refuses, and this test is
    /// what says so.
    #[test]
    fn the_panels_write_gate_agrees_with_what_a_real_write_actually_does() {
        // One well-formed gesture per writable space, in the panel's own spelling — `write_params`, not
        // a hand-built payload, so a params bug in the panel fails here rather than only on a human's
        // screen. (It already caught two: a `bytes` payload missing its D9 `0x`, and a `raw` sent as a
        // hex string where the row wants a number.)
        let gestures = [
            (Space::Bus, 0x00FF_0000u32, "AA"),
            (Space::Vram, 0x0000_0100, "55"),
            (Space::Cram, 0x0000_0020, "0EEE"),
            (Space::Z80, 0x0000_0100, "77"),
        ];

        for paused in [false, true] {
            let mut sys = booted();
            let mut b = bus(&mut sys, paused);
            let gates = probe_all_gates(&mut b, &mut sys);

            for (space, addr, payload) in gestures {
                let i = Space::ALL.iter().position(|s| *s == space).unwrap();
                let gate = &gates[i];
                let params = write_params(space, addr, payload).unwrap_or_else(|e| {
                    panic!("{}: the panel refused its own gesture: {e}", space.label())
                });
                let method = space.write_method().unwrap();
                let answer = b.call(&mut sys, method, &params);

                match (gate.is_open(), &answer) {
                    (true, Answer::Ok(_)) | (false, Answer::Err(_)) => {}
                    (true, Answer::Err(e)) => panic!(
                        "{} (paused={paused}): the gate said {method} would take this write and it \
                         answered {} {}. The panel would have offered a human a control that refuses.",
                        space.label(),
                        e.code,
                        e.message
                    ),
                    (false, Answer::Ok(v)) => panic!(
                        "{} (paused={paused}): the gate said {} but {method} took the write anyway \
                         ({v}). The panel would have greyed out a control that works — and the write \
                         LANDED while the cell claimed it could not.",
                        space.label(),
                        gate.why()
                    ),
                }
                // …and when it refused, it refused for the state, not for the shape. A gate that read a
                // params error as `machineRunning` would be right by accident here.
                if let (false, Answer::Err(e)) = (gate.is_open(), &answer) {
                    assert_eq!(
                        e.code,
                        code::INVALID_STATE,
                        "{}: the closed gate must be the run-state refusal, got {} {}",
                        space.label(),
                        e.code,
                        e.message
                    );
                    assert_eq!(answer.reason(), Some("machineRunning"));
                }
            }
        }
    }

    /// ⚑ **The asymmetry itself, named.** Three writes are paused-only and `write_vram` is not, so on a
    /// running machine the panel offers VRAM and refuses work RAM — and that is the *server's* shape,
    /// which this panel reflects rather than smooths.
    ///
    /// Asserted as an exact partition rather than as "at least one differs", because the interesting
    /// failure is either half moving: a `write_vram` that grew a gate, or one of the three that lost
    /// one. Both would be silent in the UI and both change what a human is allowed to do mid-frame.
    #[test]
    fn on_a_running_machine_vram_is_writable_and_the_other_three_are_not() {
        let mut sys = booted();
        let mut b = bus(&mut sys, false);
        let gates = probe_all_gates(&mut b, &mut sys);
        let open: Vec<&'static str> = Space::ALL
            .iter()
            .zip(gates.iter())
            .filter(|(_, g)| g.is_open())
            .map(|(s, _)| s.label())
            .collect();
        assert_eq!(
            open,
            vec!["vram"],
            "the paused-write asymmetry has MOVED. This panel reflects the server's rule and does not \
             own one, so a change here is a change on the bus (or in the contract's §6 run-control \
             rule) and needs to be understood before this expectation is edited."
        );
        // And vsram is the fifth space precisely because nothing writes it — a different fact from
        // "refused right now", and rendered as a different sentence.
        assert_eq!(gates[3], Gate::NoMethod, "vsram has no write row");
    }

    /// **Every gate says why, in words, including the open one.** A disabled control that explains
    /// nothing is indistinguishable from a broken one, and the requirement is not "non-empty" — the
    /// sentence has to name the thing a human would go looking for.
    #[test]
    fn every_write_cell_states_its_reason_and_names_its_method() {
        for paused in [false, true] {
            let mut sys = booted();
            let mut b = bus(&mut sys, paused);
            for (space, gate) in Space::ALL.iter().zip(probe_all_gates(&mut b, &mut sys)) {
                let why = gate.why();
                assert!(
                    why.len() > 20,
                    "{}: {why:?} is not an explanation",
                    space.label()
                );
                match space.write_method() {
                    Some(m) => assert!(
                        why.contains(m),
                        "{}: the reason must name the method a human would reach for, got {why:?}",
                        space.label()
                    ),
                    None => assert!(
                        why.contains("no served method"),
                        "{}: {why:?}",
                        space.label()
                    ),
                }
                if let Gate::Refused {
                    message, reason, ..
                } = &gate
                {
                    // The handler's own sentence, not a paraphrase of it.
                    assert!(
                        why.contains(message.as_str()) && why.contains(reason.as_str()),
                        "the cell must show the refusal a tool would get, verbatim: {why:?}"
                    );
                    assert!(
                        message.contains("emulator/pause"),
                        "the handler's message names the fix and the panel passes it through: \
                         {message:?}"
                    );
                }
            }
        }
    }

    /// The address box **is** `emulator/lookup_symbol` (design §2.2): it shows the tool's own answer and
    /// the tool's own refusal, and it never invents either.
    #[test]
    fn the_address_box_answers_with_the_tools_own_reply_and_refusal() {
        let mut sys = booted();

        // No listing loaded: the refusal must be the server's `-32012`, not a sentence this panel wrote.
        {
            let mut b = bus(&mut sys, false);
            match resolve_address(&mut b, &mut sys, Space::Bus, "Boot") {
                Resolved::Refused(e) => assert_eq!(
                    e.code,
                    code::NO_SYMBOLS_LOADED,
                    "no table loaded must be the server's own -32012: {} {}",
                    e.code,
                    e.message
                ),
                _ => panic!("a name with no listing loaded must be refused by the server"),
            }
        }

        // With one, the box resolves — and lands on the address the listing names.
        let listing = "  Symbol Table (* = unused):\n\n Boot : 300 C |\n\n   1 symbols\n";
        let table = oracle_core::symbols::SymbolTable::parse(listing).expect("parsable");
        let mut b = Bus::new(
            &mut sys,
            MachineInfo {
                rom_path: Some("testrom".into()),
                symbols: Some(table),
                symbols_path: Some("testrom.lst".into()),
            },
            false,
        );
        match resolve_address(&mut b, &mut sys, Space::Bus, "Boot") {
            Resolved::Symbol { addr, reply } => {
                assert_eq!(addr, 0x300);
                assert_eq!(reply["name"], json!("Boot"));
            }
            _ => panic!("`Boot` must resolve once the listing is loaded"),
        }
        // A name that is not there is the *other* refusal, and the two must stay tellable apart (§4).
        match resolve_address(&mut b, &mut sys, Space::Bus, "NoSuchThing") {
            Resolved::Refused(e) => assert_eq!(e.code, code::SYMBOL_NOT_FOUND),
            _ => panic!("an absent name must be refused"),
        }
        // Hex never dispatches: it is not a question the server can answer better.
        assert!(matches!(
            resolve_address(&mut b, &mut sys, Space::Bus, "$FF0000"),
            Resolved::Hex(0x00FF_0000)
        ));

        // ⚑ And a symbol in a VDP space is refused **by the server**, in the server's words — the panel
        // sends a real `emulator/read` rather than composing a second sentence about a rule it does not
        // own.
        match resolve_address(&mut b, &mut sys, Space::Vram, "Boot") {
            Resolved::Refused(e) => {
                assert_eq!(e.code, code::INVALID_PARAMS);
                assert!(
                    e.message.contains("symbol") && e.message.contains("bus"),
                    "the server's own rule, passed through: {:?}",
                    e.message
                );
            }
            _ => panic!("a symbol in a VDP space must be refused"),
        }
    }

    /// The hash button hands the tool a range and shows the tool's answer.
    #[test]
    fn the_hash_button_hashes_what_emulator_memory_hash_hashes() {
        let mut sys = booted();
        let mut b = bus(&mut sys, false);
        let a = hash(&mut b, &mut sys, 0x00FF_0000, 256);
        let v = ok(a);
        assert_eq!(v["len"], json!(256));
        assert_eq!(v["region"], json!("work RAM"));
        assert!(v["fnv1a64"].is_string() && v["crc32"].is_string());
        // The button is offered only where the row can name the space, and the reason is derived from
        // the row's own closed params set rather than written down here.
        assert!(hash_gate(Space::Bus).is_ok());
        let why = hash_gate(Space::Vram).expect_err("vram cannot be hashed by this row");
        assert!(
            why.contains("no `space` param"),
            "the reason must be the derived one: {why:?}"
        );
        assert!(
            !METHODS
                .iter()
                .find(|m| m.name == "emulator/memory_hash")
                .expect("served")
                .params
                .contains(&"space"),
            "if this row ever grows a `space` key, `hash_gate` opens by itself and this expectation is \
             the thing that must be revisited"
        );
    }

    /// A page that runs off the end of a sized space is **shortened loudly**, never quietly. A hex view
    /// showing four rows where sixteen were asked for, with nothing said, is a clipped read wearing a
    /// UI.
    #[test]
    fn a_page_that_runs_past_the_end_of_a_space_says_so() {
        let sys = booted();
        // CRAM is 128 bytes; a 16×16 page from $60 cannot be whole.
        let v = view(Space::Cram, &sys, 0x60, ROWS, PER_ROW);
        assert!(v.error.is_none(), "the readable part is still shown");
        assert_eq!(
            v.truncated_to,
            Some(sys.vdp().cram().len() - 0x60),
            "the shortfall must be reported, not absorbed"
        );
        assert_eq!(v.rows.len(), 2, "$60..$80 is two 16-byte rows");
        // …and past the end entirely is a refusal with a reason, never an empty grid.
        let past = view(Space::Cram, &sys, 0x400, ROWS, PER_ROW);
        assert!(past.rows.is_empty());
        let e = past.error.expect("past the end must refuse");
        assert_eq!(e.code, code::ADDRESS_OUT_OF_RANGE);
        assert!(
            e.message.contains("cram"),
            "the message names the space: {}",
            e.message
        );
    }

    /// A read the *bus* refuses is refused identically here, with the handler's own message — the panel
    /// does not invent a bound of its own for the space that has no single end.
    #[test]
    fn a_bus_read_off_the_end_of_a_region_refuses_exactly_as_the_tool_does() {
        let mut sys = booted();
        let mut b = bus(&mut sys, false);
        let addr = 0x00FF_FFF0u32;
        let panel = read(Space::Bus, &sys, addr, 32).expect_err("this runs past work RAM");
        let served = match b.call(
            &mut sys,
            "emulator/read",
            &json!({"space": "bus", "addr": oracle_aether::hex::addr(addr), "len": 32}),
        ) {
            Answer::Err(e) => e,
            Answer::Ok(v) => panic!("the bus accepted a read past the end of work RAM: {v}"),
        };
        assert_eq!((panel.code, panel.message), (served.code, served.message));
        assert_eq!(panel.code, code::ADDRESS_OUT_OF_RANGE);
    }

    /// The Z80 window's `$2000-$3FFF` mirror is the machine's, and the panel folds it through the same
    /// function the handler does rather than through a second copy of the mask.
    #[test]
    fn the_z80_mirror_folds_the_same_way_for_the_panel_and_the_tool() {
        let mut sys = booted();
        let mut b = bus(&mut sys, true);
        ok(b.call(
            &mut sys,
            "emulator/z80_write",
            &json!({"addr": "0x00000123", "bytes": "0xC3"}),
        ));
        let (low, _) = read(Space::Z80, &sys, 0x0123, 1).expect("in window");
        let (mirrored, _) = read(Space::Z80, &sys, 0x2123, 1).expect("in window");
        assert_eq!(low, vec![0xC3]);
        assert_eq!(mirrored, low, "$2123 mirrors $0123 — that is the machine");
        // …and the window is bounded at BOTH ends, refused whole rather than wrapped.
        let e = read(Space::Z80, &sys, 0x3FFF, 2).expect_err("this runs past the window");
        assert_eq!(e.code, code::ADDRESS_OUT_OF_RANGE);
    }
}
