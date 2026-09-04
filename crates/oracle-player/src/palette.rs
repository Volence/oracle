//! **The command palette — the home for the half of this window that DOES things.**
//!
//! The standing ruling this module implements, in the owner's words: *"Things you LOOK AT — registers,
//! memory, objects, breakpoints, profiler — are tabs. Things you DO — reset, press, spawn, write — are NOT
//! tabs. They are controls inside a panel or an invoked command. A tab that is empty until used is a worse
//! button."*
//!
//! [`crate::ui::Tab`] is the first half. The transport bar is three of the second half, hand-placed
//! because a human reaches for pause/resume/step constantly. This module is the rest: **every method this
//! build serves, reachable from the window, without a tab and without a tool.** Without it the shipped
//! surface offers exactly five mutating commands out of the registry's many, and the lane's standing
//! default — *a capability served on the bus is reachable in the window* — is false by omission.
//!
//! # ⚑ The list is DERIVED from the registry. It is not a list.
//!
//! [`offered`] maps over [`oracle_aether::engine::METHODS`] and borrows its rows. There is no array of
//! method names in this file and there must never be one:
//!
//! * A hardcoded menu goes stale **silently**. The window would then confidently offer the capabilities of
//!   a server that does not exist — a believable wrong answer, which is the class this repo's cutover
//!   ruling is written against, not a missing one.
//! * The registry already carries everything a palette needs to render a row: [`MethodSpec::name`] is the
//!   wire name, [`MethodSpec::summary`] is the one-line description `initialize` advertises, and
//!   [`MethodSpec::params`] is the **closed** top-level key set `Engine::dispatch` enforces *before* the
//!   handler runs. Three fields, all authoritative, none of them ours.
//! * So a method added to the engine appears here on the next build, and one removed disappears. Neither
//!   costs an edit in this file, which is the property [`the_palette_offers_the_registry_and_nothing_else`]
//!   exists to keep true.
//!
//! # ⚑ In-process, never a socket to itself
//!
//! Contract D15: *"An in-process GUI is a consumer of the same registry, not a second server … it reads
//! the method registry directly, in-process; it does not open a socket to itself."* Every command this
//! palette issues goes through [`crate::bus::Bus::call`] — `Host::call`, synchronous, answering against
//! the machine handed in — so a command gets **the tool's exact reply and the tool's exact refusal**.
//!
//! [`oracle_aether::host::Host::pump`] is deliberately not the path: it is the once-per-iteration drain,
//! so routing a gesture through it would make a keystroke wait a frame before it was even dispatched. That
//! is the same call `crate::ui::Transport` already made for the transport bar and `oracle-frontend`'s
//! spawn picker made for its click; this is the third site and it is not a new decision.
//!
//! # ⚑ A refusal arrives as a sentence, and the sentence is the server's
//!
//! [`crate::ui::Echo`] carries `code` and `message` **verbatim** and colours on `refused` rather than on
//! the shape of the string. This module adds exactly one thing on top of it: a [`remedy`], keyed on the
//! machine-readable `error.data.reason` and never on the message text, naming a control **this window
//! actually has** — spelled from [`crate::ui::PAUSE_LABEL`] rather than transcribed, so rewording the
//! button cannot leave the remedy pointing at a label nobody draws.
//!
//! ⚑ **`oracle-frontend`'s [`Refusal`](../../oracle-frontend/src/spawn.rs) was NOT lifted, and the reason
//! is structural rather than taste.** `oracle-frontend` is a binary crate with no `lib` target (its own
//! `audio.rs` is `#[path]`-included here for exactly that reason), so there is nothing in it to depend on;
//! and its remedy vocabulary is a minifb key binding (*"press Space to pause this window"*), which names a
//! control this window does not have. What generalised is the **rule** — verbatim message, remedy keyed on
//! `reason`, colour from a flag and not from prose — and the rule was already implemented in this crate by
//! `Echo`. Copying the type would have put a second spelling of one decision on disk, which is the drift
//! R2 exists to prevent.
//!
//! # ⚑ Failing loudly, by name
//!
//! Two refusals are the **window's own**, raised before a call:
//!
//! * A typed name the registry does not carry ([`resolve`]). It is refused with the string the human typed
//!   and **the served count derived from `METHODS.len()`**, never a typed integer. A palette that quietly
//!   did nothing on an unknown command is the precise failure this parcel exists to not ship.
//! * A malformed argument ([`parse_args`]). `serde_json`'s **own** parse error is shown — line, column and
//!   all — rather than "invalid JSON", because the human's next action is fixing a character and a summary
//!   deletes the coordinate.
//!
//! Everything else is the server's. A well-formed call to a served method whose params are wrong comes
//! back `-32602` from `Engine::dispatch`'s closed-key check **before the handler runs**, so a write refused
//! for an unknown param has written nothing — and that sentence reaches the glass whole.
//!
//! # What this deliberately is NOT
//!
//! **No per-parameter form generator.** A method that needs params takes a JSON object typed into one box.
//! Generating a widget per param means reading the vendored schema at runtime for types, ranges and the
//! `oneOf` alternatives half these methods carry (`write_memory`'s `bytes`/`value`, `write_cram`'s
//! `r,g,b`/`raw`) — a bigger parcel with its own staleness question, and not this one. The declared key set
//! is *shown* beside the box, which is the cheap half of the same help.
//!
//! **No layout persistence.** The palette is not a [`crate::ui::Tab`] and stores nothing across launches,
//! for the standing reason: saving a layout of controls buys a migration.

use egui::{Key, KeyboardShortcut, Modifiers};
use oracle_aether::engine::{MethodSpec, METHODS};
use serde_json::Value;

use crate::bus::{Answer, Bus};
use crate::machine::Machine;
use crate::screen;
use crate::ui::Echo;

/// The top bar's label for the control that opens this, and the palette window's own title.
///
/// A constant for [`crate::ui::PAUSE_LABEL`]'s reason: [`crate::screen`] reports the bar over
/// `emulator/screen_text`, and a label spelled twice is a window and a client naming one control two ways.
pub const PALETTE_LABEL: &str = "⌨ commands";

/// **The keystroke that opens it.** `Ctrl+P` — the palette convention, and unbound elsewhere in this
/// window (the pad reads letter keys through [`crate::input`], which is gated off entirely whenever egui
/// holds keyboard focus, so a modifier chord cannot be mistaken for a d-pad press).
pub const SHORTCUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::P);

/// How [`SHORTCUT`] is written for a human, once, so the button's hover text and any prose about it cannot
/// disagree with the binding above.
pub const SHORTCUT_LABEL: &str = "Ctrl+P";

/// **Every method this build serves, filtered by what the human has typed.**
///
/// Borrowed rows of [`METHODS`], never copies: the palette shows the registry's own `name`, `summary` and
/// `params`, so there is no second description of a method anywhere in this crate to go stale.
///
/// The filter is a case-insensitive substring over the name **and** the summary — a human hunting for
/// "the profiler thing" types `profiler`, a human hunting for "how do I poke memory" types `poke` and
/// finds `emulator/write_memory` through its summary. An empty query offers everything, which is the
/// honest default for a surface whose whole point is *what can this thing do*.
pub fn offered(query: &str) -> Vec<&'static MethodSpec> {
    let needle = query.trim().to_ascii_lowercase();
    METHODS
        .iter()
        .filter(|m| {
            needle.is_empty()
                || m.name.to_ascii_lowercase().contains(&needle)
                || m.summary.to_ascii_lowercase().contains(&needle)
        })
        .collect()
}

/// **Turn a typed string into a served method, or say — by name — that it is not one.**
///
/// The `Err` is the *window's* refusal, raised before any call, and it names two things a human can act on:
/// the string they typed, and how many methods this build actually serves. The count comes from
/// `METHODS.len()` at the moment of the refusal. It is **derived, never pinned**: a consumer that typed the
/// number would turn its own staleness guard into the stale thing.
///
/// An exact match only. A palette that "helpfully" ran the nearest name would be inventing an intent, and
/// the intent it invents could be `emulator/reset`.
pub fn resolve(typed: &str) -> Result<&'static MethodSpec, String> {
    let name = typed.trim();
    METHODS.iter().find(|m| m.name == name).ok_or_else(|| {
        format!(
            "no method named `{name}` is served by this build — {} are, and the list above is all of \
             them. Nothing was sent.",
            METHODS.len()
        )
    })
}

/// **Parse the argument box.** Empty means `{}` — most methods take no params and typing `{}` for them
/// would be ceremony.
///
/// Two ways to fail, and both name what is wrong rather than that something is:
///
/// * Not JSON at all — `serde_json`'s own message, which carries the line and column. Quoted whole, for
///   [`Echo`]'s reason: a parse error summarised as "invalid JSON" deletes the coordinate that was the
///   entire content of the answer.
/// * Valid JSON that is not an object — refused here rather than sent, because `params` is an object on
///   this bus by construction (`MethodSpec::params` is a key set) and a `[1,2,3]` reaching the engine
///   would be answered by a generic deserialiser error about a shape the human never intended.
pub fn parse_args(text: &str) -> Result<Value, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(Value::Object(Default::default()));
    }
    match serde_json::from_str::<Value>(t) {
        // The parser's own words, whole: line and column included. Nothing was sent.
        Err(e) => Err(format!("that is not JSON: {e}. Nothing was sent.")),
        Ok(v) if !v.is_object() => Err(format!(
            "arguments must be a JSON object like {{\"addr\": \"0xFF0000\"}} — this is {}. Nothing was \
             sent.",
            match &v {
                Value::Null => "null",
                Value::Bool(_) => "a boolean",
                Value::Number(_) => "a number",
                Value::String(_) => "a string",
                Value::Array(_) => "an array",
                Value::Object(_) => unreachable!("guarded by the arm above"),
            }
        )),
        Ok(v) => Ok(v),
    }
}

/// **What this window can do about a refusal**, keyed on the machine-readable discriminant, or `None`.
///
/// Keyed on `error.data.reason` — §5 is explicit that the message text is for humans and the reason is for
/// code, and a remedy that matched on prose would break on a wording fix.
///
/// `machineRunning` is the only entry, and that is the honest list rather than a thin one. Fifteen served
/// methods refuse a running machine, this window owns the control that fixes it, and the label is read from
/// [`crate::ui::PAUSE_LABEL`] rather than retyped — so a rewording of the button moves this sentence with
/// it. The other discriminants on this bus (`noDisplay`, `unknownCheckpoint`, `objectPoolFull`,
/// `callersNotArmed`, `perFrameNotArmed`) are answered by the machine's state or by a different call, not
/// by a control on this bar, and inventing prose for them would be this module guessing.
pub fn remedy(reason: Option<&str>) -> Option<String> {
    match reason {
        Some("machineRunning") => Some(format!(
            "the machine is running — press `{}` on the top bar (or run `{}` from here), then try again",
            crate::ui::PAUSE_LABEL,
            crate::ui::PAUSE,
        )),
        _ => None,
    }
}

/// One line of the palette's own refusal — the window's, not the server's.
///
/// Kept apart from [`Echo`] rather than folded into it, because the two are different facts and a reader
/// is told which they are looking at: an `Echo` means *the bus answered*, a `Local` means **nothing was
/// sent**. Collapsing them would let "the server refused" and "I never asked" render identically, which is
/// the one distinction a debug surface cannot afford to lose.
pub struct Local(pub String);

/// The palette's state between repaints.
///
/// Deliberately holds **no copy of the method list** — [`offered`] re-derives it from [`METHODS`] every
/// repaint, which is free (a filter over a `&'static` slice) and cannot go stale. It holds only what the
/// human has typed and what they last got back.
#[derive(Default)]
pub struct Palette {
    /// Whether the window is up. Not persisted: see the module header.
    pub open: bool,
    /// The filter box, which is also the command line — an exact match here is what [`resolve`] runs.
    pub query: String,
    /// The JSON argument object, as typed.
    pub args: String,
    /// The last answer from the bus, in the server's own words. `None` before the first command.
    pub last: Option<Echo>,
    /// The last refusal **this window** raised — an unknown name or a malformed argument. Cleared whenever
    /// a call actually goes out, so a stale "nothing was sent" cannot sit beside a reply that was.
    pub local: Option<Local>,
}

impl Palette {
    /// Consume [`SHORTCUT`] if it was pressed this frame, and toggle.
    ///
    /// `consume_shortcut` rather than a raw key read so the chord does not also reach whatever has focus.
    pub fn handle_shortcut(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_shortcut(&SHORTCUT)) {
            self.open = !self.open;
        }
    }

    /// **Issue one command and keep its answer**, in the server's words.
    ///
    /// The two window-side refusals are raised here, before the call, and each returns early with
    /// [`Self::local`] set and [`Self::last`] untouched-but-cleared — so the glass never shows a reply from
    /// an earlier command beside a refusal to send this one.
    ///
    /// Everything past those two goes to [`Bus::call`] and whatever comes back is rendered whole.
    pub fn run(&mut self, machine: &mut Machine, bus: &mut Bus, typed: &str, args: &str) {
        self.last = None;
        self.local = None;
        let spec = match resolve(typed) {
            Ok(s) => s,
            Err(e) => {
                self.local = Some(Local(e));
                return;
            }
        };
        let params = match parse_args(args) {
            Ok(p) => p,
            Err(e) => {
                self.local = Some(Local(e));
                return;
            }
        };
        let answer = bus.call(machine.system_mut(), spec.name, &params);
        self.last = Some(Echo {
            method: spec.name,
            refused: answer.is_err(),
            reason: answer.reason().map(str::to_string),
            text: match &answer {
                // The reply whole, not a summary: a palette is where a human goes to see what a method
                // actually returns, and truncating it here would send them to the tool they came from.
                Answer::Ok(v) => format!("ok {v}"),
                Answer::Err(e) => format!("{} {}", e.code, e.message),
            },
        });
    }

    /// **Draw the palette**, and return the text runs it put on the glass for `emulator/screen_text`.
    ///
    /// Returns runs for the same reason [`crate::ui::Transport::bar`] does: handing back what was drawn is
    /// a different guarantee from computing the same thing twice, and a modal covering this window whose
    /// text a client could not read would be a hole in the readback rather than a saving.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        machine: &mut Machine,
        bus: &mut Bus,
    ) -> Vec<screen::Run> {
        let mut drew = Vec::new();
        if !self.open {
            return drew;
        }
        // Taken before the closure so the borrow of `self` inside it stays disjoint from the run below.
        let mut fire: Option<(String, String)> = None;
        let mut open = self.open;
        egui::Window::new(PALETTE_LABEL)
            .open(&mut open)
            .default_width(560.0)
            .show(ctx, |ui| {
                let rows = offered(&self.query);
                // **The headline is derived twice over and pinned nowhere**: how many rows match, out of
                // how many the build serves.
                let head = format!(
                    "{} of {} served methods — in-process, through the same registry a tool reads (D15)",
                    rows.len(),
                    METHODS.len()
                );
                ui.weak(&head);
                drew.push(screen::Run::label(head));

                ui.horizontal(|ui| {
                    ui.label("method");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .hint_text("emulator/…  — filter, or type a full name and press Run")
                            .desired_width(340.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("params");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.args)
                            .hint_text("{} — a JSON object, or empty for none")
                            .desired_width(340.0),
                    );
                    if ui.button("Run").clicked() {
                        fire = Some((self.query.clone(), self.args.clone()));
                    }
                });

                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for m in &rows {
                            ui.horizontal(|ui| {
                                // Clicking a row loads it into the command line rather than running it.
                                // A palette whose list items fire on click is one mis-click away from
                                // `emulator/reset` on a session somebody was ten minutes into.
                                if ui.button(m.name).on_hover_text(m.summary).clicked() {
                                    self.query = m.name.to_string();
                                }
                                ui.weak(m.summary);
                            });
                            // The declared key set, from the registry. `Engine::dispatch` refuses any
                            // top-level key not in this list with `-32602` **before the handler runs**, so
                            // this is not a hint about what is likely — it is the closed set.
                            if !m.params.is_empty() {
                                ui.weak(format!("      params: {}", m.params.join(", ")));
                            }
                        }
                    });

                if let Some(Local(msg)) = &self.local {
                    ui.separator();
                    // Coloured on being a refusal, never on the shape of the string.
                    ui.colored_label(ui.visuals().error_fg_color, msg)
                        .on_hover_text("this window's own refusal — the command never reached the bus");
                    drew.push(screen::Run::after_sep(msg.clone()));
                }
                if let Some(e) = &self.last {
                    ui.separator();
                    let colour = if e.refused {
                        ui.visuals().error_fg_color
                    } else {
                        ui.visuals().weak_text_color()
                    };
                    let line = e.line();
                    ui.colored_label(colour, &line).on_hover_text(
                        "the bus's own reply, verbatim. The bracketed word is `error.data.reason`, the \
                         discriminant clients branch on — never the message text.",
                    );
                    drew.push(screen::Run::after_sep(line));
                    if let Some(r) = remedy(e.reason.as_deref()) {
                        ui.weak(&r);
                        drew.push(screen::Run::label(r));
                    }
                }
            });
        self.open = open;
        if let Some((typed, args)) = fire {
            self.run(machine, bus, &typed, &args);
        }
        drew
    }
}

// ---------------------------------------------------------------------------------------------------

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use oracle_core::system::System;
    use serde_json::json;

    fn rig() -> (Machine, Bus) {
        let mut machine = Machine::new(oracle_core::testrom::build(), None);
        let bus = Bus::new(
            machine.system_mut(),
            oracle_aether::host::MachineInfo::default(),
            // Paused, so the write gates below are exercised in the state a human would be in when they
            // reach for a poke. The running case is its own test.
            true,
            None,
        );
        (machine, bus)
    }

    /// Work RAM, read **straight off the emulated machine**, with no bus and no `oracle_aether` in the
    /// path.
    ///
    /// This is the independent channel the effect gates anchor on. `engine::debug_read` would have been
    /// one line shorter and would have been the *served side's own account of itself* — a reply and a
    /// readback derived from the same code, agreeing perfectly while both were wrong.
    /// [`System::ram`] is the 64 KiB the 68000 actually addresses at `$FF0000`.
    fn ram_at(sys: &System, addr: u32, len: usize) -> Vec<u8> {
        let base = (addr & 0xFFFF) as usize;
        sys.ram()[base..base + len].to_vec()
    }

    /// ★ **The palette offers the registry — the whole registry, and nothing that is not in it.**
    ///
    /// This is the parcel's load-bearing property. A hardcoded list would satisfy a human reading the
    /// window and would then, on the first method added or removed, offer the capabilities of a server
    /// that does not exist.
    ///
    /// **⚑ The third assertion, and why the first two are not enough.** Rows one and two are a parity pair
    /// over one shared derivation: [`offered`] reads `METHODS` and so does the ruler, so a break in
    /// `METHODS` moves both sides together and the pair agrees perfectly while both are wrong. So the
    /// registry is asserted to be **non-empty and to actually contain the method the transport bar has
    /// been issuing since parcel 3** — an anchor outside this module's own arithmetic. An empty registry
    /// would otherwise make `[] == []` pass here forever.
    #[test]
    fn the_palette_offers_the_registry_and_nothing_else() {
        assert!(
            !METHODS.is_empty(),
            "the registry is empty, so every count comparison below is `0 == 0` and witnesses nothing"
        );
        assert!(
            METHODS.iter().any(|m| m.name == crate::ui::PAUSE),
            "`{}` is not in the registry — either the transport bar has been issuing a method that does \
             not exist, or this ruler is reading the wrong table",
            crate::ui::PAUSE
        );

        let all = offered("");
        assert_eq!(
            all.len(),
            METHODS.len(),
            "an empty query must offer the whole registry"
        );
        // **Identity by content, across ALL THREE fields, in registry order** — not just a count.
        //
        // ⚑ This was written as `std::ptr::eq` first, and it FAILED on a correct implementation. The
        // reason is worth keeping: `METHODS` is a `pub const`, not a `static`, so const-promotion
        // materialises a **separate allocation per MIR body**. `offered`'s rows and this test's rows are
        // therefore at different addresses even though neither is a copy anybody wrote. (Measured: three
        // probes comparing `METHODS` against itself *inside one function* all passed; the same comparison
        // across two functions in this same module failed.) Pointer identity is simply not available as a
        // proof while the registry is a `const` — see the report note about promoting it to a `static`.
        //
        // Content over all three fields is the strongest proof that remains, and it is a strong one: a
        // hardcoded menu could only satisfy it by transcribing every name, every summary and every
        // declared param set in registry order, which is a copy that cannot drift without turning this
        // red — the whole property the module header claims.
        for (row, spec) in all.iter().zip(METHODS.iter()) {
            assert_eq!(
                (row.name, row.summary, row.params),
                (spec.name, spec.summary, spec.params),
                "`offered` handed back a row that is not the registry's description of `{}` — a second \
                 description of a method is the thing that goes stale",
                spec.name
            );
        }
    }

    /// ★ **The filter actually filters** — the assertion that proves the derivation above did something.
    ///
    /// Without this, an `offered` that ignored its argument and returned `METHODS.iter().collect()`
    /// unconditionally would satisfy the count test perfectly. The needle is derived from the registry
    /// itself (a name that exists) rather than typed, and the expectation is `< all`, never a number.
    #[test]
    fn the_filter_narrows_rather_than_returning_everything() {
        let all = offered("");
        let narrowed = offered("profiler");
        assert!(
            !narrowed.is_empty(),
            "`profiler` matched nothing, so the comparison below is vacuous — either the registry lost \
             its profiler rows or the filter is broken"
        );
        assert!(
            narrowed.len() < all.len(),
            "the filter returned {} of {} rows, i.e. it did not filter — an `offered` that ignores its \
             query satisfies every count test in this module",
            narrowed.len(),
            all.len()
        );
        assert!(
            narrowed.iter().all(|m| m.name.contains("profiler")
                || m.summary.to_ascii_lowercase().contains("profiler")),
            "a row matched `profiler` through neither its name nor its summary"
        );
        // Case-insensitivity is a claim this module makes; a claim not asserted is a comment.
        assert_eq!(offered("PROFILER").len(), narrowed.len());
    }

    /// ★ **An unknown command fails loudly, by name, and never silently does nothing.**
    ///
    /// The exact failure this parcel exists to not ship. The refusal must carry the string the human typed
    /// — so they can see the typo — and the served count, **derived** so it cannot become the stale thing.
    ///
    /// **⚑ The third assertion.** A `resolve` that returned `Err` unconditionally satisfies the rows above,
    /// so a name taken *out of the registry* is asserted to resolve `Ok` and to the same row.
    #[test]
    fn an_unknown_command_is_refused_by_name_and_nothing_is_sent() {
        let typo = "emulator/definitely_not_a_served_method";
        let err = match resolve(typo) {
            Err(e) => e,
            Ok(m) => panic!("`{typo}` must not resolve; it found `{}`", m.name),
        };
        assert!(
            err.contains(typo),
            "the refusal must name what was typed; it said: {err}"
        );
        assert!(
            err.contains(&METHODS.len().to_string()),
            "the refusal must carry the served count derived from `METHODS.len()` ({}); it said: {err}",
            METHODS.len()
        );
        assert!(
            err.to_ascii_lowercase().contains("nothing was sent"),
            "a window-side refusal must say the command never reached the bus; it said: {err}"
        );

        let known = METHODS[0].name;
        let ok = match resolve(known) {
            Ok(m) => m,
            Err(e) => panic!("a registry name must resolve; it said: {e}"),
        };
        // Content, not pointer — see the note in `the_palette_offers_the_registry_and_nothing_else` for
        // why `ptr::eq` cannot answer this while `METHODS` is a `const`.
        assert_eq!(
            (ok.name, ok.summary, ok.params),
            (METHODS[0].name, METHODS[0].summary, METHODS[0].params),
            "`resolve` must hand back the registry's own row, not a lookalike"
        );
        // And an approximate match is NOT run: a palette that guessed could guess `emulator/reset`.
        let prefix = &known[..known.len() - 1];
        assert!(
            resolve(prefix).is_err(),
            "`{prefix}` is not a served name and must not resolve to `{known}` — a palette that ran the \
             nearest match would be inventing an intent"
        );
    }

    /// ★ **A malformed argument is refused with the parser's own words, not a canned string.**
    ///
    /// The human's next action is fixing a character, and the line/column is the entire content of that
    /// answer — so the expectation is derived by asking `serde_json` the same question, never transcribed.
    ///
    /// **⚑ The third assertion.** Valid JSON must parse, or a `parse_args` that refused everything would
    /// satisfy the rows above; and empty must mean `{}`, which is the affordance most rows depend on.
    #[test]
    fn a_malformed_argument_is_refused_in_the_parser_s_own_words() {
        let bad = "{addr: 0xFF0000";
        let theirs = serde_json::from_str::<Value>(bad)
            .expect_err("this is not JSON")
            .to_string();
        let ours = parse_args(bad).expect_err("`parse_args` must refuse it");
        assert!(
            ours.contains(&theirs),
            "the parser's own message must arrive whole. serde said `{theirs}`; the palette said `{ours}`"
        );
        assert!(
            theirs.contains("line") && theirs.contains("column"),
            "serde no longer reports a coordinate ({theirs}), so this test is asserting a property the \
             error no longer has — re-derive it rather than weakening it"
        );

        // Valid JSON that is not an object is refused here rather than sent as a shape the bus cannot take.
        let arr = parse_args("[1,2,3]").expect_err("an array is not a params object");
        assert!(
            arr.contains("array"),
            "the refusal must name the shape: {arr}"
        );

        assert_eq!(
            parse_args("{\"addr\": \"0xFF0000\"}").expect("valid JSON must parse"),
            json!({"addr": "0xFF0000"}),
            "a `parse_args` that refused everything would satisfy every row above"
        );
        assert_eq!(
            parse_args("   ").expect("empty must mean no params"),
            json!({}),
            "empty must mean `{{}}` — most served methods take no params and typing braces is ceremony"
        );
    }

    /// ★★ **A command run from the palette moves the EMULATED MACHINE — asserted against work RAM, not
    /// against the reply.**
    ///
    /// The anchor is [`ram_at`], which reads `System::ram()` with no `oracle_aether` in the path. That is
    /// the whole point: `emulator/write_memory`'s reply echoes the request, so a palette that dispatched
    /// the wrong method — or the right method with the wrong bytes — can still return a reply that reads
    /// perfectly correct. Only an independent witness catches it.
    ///
    /// **⚑ The third assertion.** The pre-state is asserted **different** from the payload before the call.
    /// Without it, a palette that did nothing at all would pass on any RAM that happened to already hold
    /// these bytes — and a fresh machine's RAM is zeros, so the payload is chosen non-zero and the
    /// `assert_ne!` proves the write, rather than the machine, produced the change.
    #[test]
    fn a_palette_command_moves_the_machine_and_ram_is_the_witness() {
        let (mut machine, mut bus) = rig();
        let mut p = Palette::default();
        const ADDR: u32 = 0xFF_0000;
        const PAYLOAD: [u8; 4] = [0xDE, 0xAD, 0xBE, 0xEF];

        let before = ram_at(machine.system(), ADDR, PAYLOAD.len());
        assert_ne!(
            before,
            PAYLOAD.to_vec(),
            "work RAM already held the payload before the call, so the `assert_eq!` below would pass \
             against a palette that did nothing"
        );

        p.run(
            &mut machine,
            &mut bus,
            "emulator/write_memory",
            "{\"addr\": \"0xFF0000\", \"bytes\": \"0xDEADBEEF\"}",
        );

        let echo = p.last.as_ref().expect("the bus must have answered");
        assert!(
            !echo.refused,
            "the write was refused, so the RAM assertion below would be testing a machine nothing \
             touched: {}",
            echo.line()
        );
        assert!(
            p.local.is_none(),
            "a window-side refusal was raised, so nothing was sent"
        );

        assert_eq!(
            ram_at(machine.system(), ADDR, PAYLOAD.len()),
            PAYLOAD.to_vec(),
            "the palette reported success and the 68000's work RAM does not hold the bytes — the reply \
             is the served side's account of itself and this is the independent channel"
        );
    }

    /// ★ **A refusal arrives as the server's sentence, byte for byte, plus a remedy this window owns.**
    ///
    /// `emulator/write_memory` against a running machine is refused `-32005 machineRunning` by
    /// `require_paused`. The palette must render that message **verbatim** and must not have written.
    ///
    /// **⚑ The third assertion, twice over.** The message is compared against one obtained from an
    /// independent direct `Bus::call` (so the expectation is the server's, not a transcription), the
    /// message is asserted non-empty (or `contains("")` passes forever), **and** work RAM is asserted
    /// unchanged — a refusal that had written anyway is a far worse defect than a wrong sentence, and the
    /// reply alone cannot see it.
    #[test]
    fn a_refusal_arrives_verbatim_and_the_machine_did_not_move() {
        let mut machine = Machine::new(oracle_core::testrom::build(), None);
        // `false` = a free-running bus, which is what fifteen methods refuse.
        let mut bus = Bus::new(
            machine.system_mut(),
            oracle_aether::host::MachineInfo::default(),
            false,
            None,
        );
        const ADDR: u32 = 0xFF_0000;
        let before = ram_at(machine.system(), ADDR, 4);

        // The server's own words, obtained independently of the palette.
        let theirs = match bus.call(
            machine.system_mut(),
            "emulator/write_memory",
            &json!({"addr": "0xFF0000", "bytes": "0xDEADBEEF"}),
        ) {
            Answer::Err(e) => e,
            Answer::Ok(v) => panic!("a running machine must refuse a poke; it replied {v}"),
        };
        assert!(
            !theirs.message.is_empty(),
            "an empty message makes the `contains` below pass forever"
        );
        assert_eq!(
            theirs.data.as_ref().and_then(|d| d.get("reason")),
            Some(&json!("machineRunning")),
            "the discriminant is what the remedy keys on; without it this test proves nothing about it"
        );

        let mut p = Palette::default();
        p.run(
            &mut machine,
            &mut bus,
            "emulator/write_memory",
            "{\"addr\": \"0xFF0000\", \"bytes\": \"0xDEADBEEF\"}",
        );
        let echo = p.last.as_ref().expect("the bus must have answered");
        assert!(echo.refused, "the palette must colour this as a refusal");
        assert!(
            echo.text.contains(&theirs.message),
            "the server's message must arrive whole. It said `{}`; the palette showed `{}`",
            theirs.message,
            echo.text
        );
        assert!(
            echo.text.contains(&theirs.code.to_string()),
            "the code must be shown beside it; the palette showed `{}`",
            echo.text
        );

        let r = remedy(echo.reason.as_deref()).expect("`machineRunning` must carry a remedy");
        assert!(
            r.contains(crate::ui::PAUSE_LABEL) && r.contains(crate::ui::PAUSE),
            "the remedy must name the control this window actually draws, spelled from the constants \
             the bar draws it with; it said: {r}"
        );
        assert_eq!(
            remedy(Some("objectPoolFull")),
            None,
            "a discriminant this window owns no control for must get no invented remedy"
        );
        assert_eq!(remedy(None), None);

        assert_eq!(
            ram_at(machine.system(), ADDR, 4),
            before,
            "the machine was refused and moved anyway"
        );
    }

    /// ★ **The window's own refusals send nothing, and are not dressed as the server's.**
    ///
    /// `Palette::last` staying `None` is the assertion that the call never happened — an unknown method
    /// that reached the bus would come back `-32601` and populate it, which reads as a server refusal for
    /// something the server never saw.
    #[test]
    fn a_window_side_refusal_leaves_the_bus_untouched() {
        let (mut machine, mut bus) = rig();
        let mut p = Palette::default();

        p.run(&mut machine, &mut bus, "emulator/not_a_method", "");
        assert!(
            p.last.is_none(),
            "an unknown name must not reach the bus — a `-32601` here would be the server answering for a \
             mistake it never saw"
        );
        assert!(p.local.is_some(), "and it must say so, loudly");

        p.run(&mut machine, &mut bus, "emulator/status", "not json");
        assert!(p.last.is_none(), "a malformed argument must not be sent");
        let Local(msg) = p.local.as_ref().expect("and it must say so");
        assert!(msg.contains("not JSON"), "it said: {msg}");

        // And a good command clears the stale window-side refusal rather than leaving it beside a reply.
        p.run(&mut machine, &mut bus, "emulator/status", "");
        assert!(
            p.local.is_none(),
            "a stale `nothing was sent` must not sit beside a reply that was"
        );
        assert!(p.last.is_some(), "a served method must answer");
    }
}
