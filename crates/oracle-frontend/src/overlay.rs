//! The on-screen overlay: transient notifications, a persistent status line, a save-state slot strip and the
//! paused indicator — drawn into the presented window buffer with [`crate::font`].
//!
//! **Why.** Everything this frontend had to say — a save confirming, the slot changing, a volume step, a
//! failed load, the console filter it picked — went to `stdout`. A person who double-clicks a window never
//! reads stdout, so from their side the emulator was silent about its own state; the owner's first real
//! session hit exactly that. Every message now goes to *both*: `println!` keeps the terminal log (and the
//! tests and the scripts that read it) exactly as it was, and a toast puts the same words on the glass.
//!
//! **The pause indicator is a correctness matter, not decoration.** Since the render path started retaining
//! the last good framebuffer, a paused frontend and a hung one look identical — the same picture, forever.
//! The `PAUSED` banner is the only thing that distinguishes them. It waits out a short **dwell** first (see
//! `PAUSED_BANNER_DWELL_FRAMES`), because the machine is now paused and resumed by clients as well as by
//! the person at the keyboard, and a banner that strobes through a write burst is worse than no banner.
//!
//! **Where things go.** Everything is anchored to the **picture**, not the window: toasts stack up from the
//! picture's bottom-left corner, the status line sits at its top-left, and the paused banner is centred
//! across it. A letterboxed window therefore keeps its black bars black, and the font scale follows the
//! picture's height so the overlay grows with the game rather than with the desktop. Nothing here touches the
//! retained native framebuffer: the overlay is drawn into the *presentation* buffer, which is rebuilt from
//! scratch every present, so a re-presented frame can never accumulate overlay ink (the bug the crosshair
//! learned the hard way).

use crate::font;
use crate::present::Rect;
use crate::save_state::SLOT_COUNT;
use oracle_core::render::LayerMask;
use std::borrow::Cow;
use std::collections::VecDeque;

/// How long a toast stays up, in presented frames (~2.5 s at 60 fps). Long enough to read a save
/// confirmation, short enough that a burst of slot presses does not bury the picture.
pub const TOAST_FRAMES: u32 = 150;

/// The last of a toast's life spent fading out, in frames (~0.5 s), so messages leave gently instead of
/// blinking off.
const FADE_FRAMES: u32 = 30;

/// How many toasts are shown at once. Older ones are dropped from the top when a burst overflows this.
pub const MAX_TOASTS: usize = 5;

/// How long the machine must have been **continuously** paused before the `PAUSED` banner appears, in
/// presented frames (~200 ms at 60 fps NTSC).
///
/// **Why a dwell at all.** The banner used to key straight off "is it paused right now", which was fine
/// while the only thing that paused the machine was a person pressing Space. It is not the only thing any
/// more: `write_memory` is `require_paused`, so an Aurora palette drag at 10 Hz pauses and resumes the
/// machine dozens of times a second and the banner strobed for the length of the drag. The owner's words:
/// *"can we have it not write PAUSE/UNPAUSE on the screen when we do something that causes a change."*
///
/// **Why this number.** Under about a fifth of a second a human reads a real pause as instantaneous, so
/// nothing is lost from the Space-bar case; and it is far longer than any sub-frame client write burst,
/// each of which resets the count on the very next present. The two constraints leave a wide gap and 12
/// frames sits in it.
///
/// **Deliberately stateless about the pause's origin.** There is no player-vs-bus distinction here, and
/// that is the point: the rule is about *duration*, which is the thing the viewer actually experiences,
/// so it covers clients this frontend has not met yet without either side having to declare itself.
const PAUSED_BANNER_DWELL_FRAMES: u32 = 12;

/// Text colours. Deliberately few: white for ordinary confirmations, amber for state the user is steering,
/// red for refusals.
pub const INFO: u32 = 0x00FF_FFFF;
pub const ACCENT: u32 = 0x00FF_C84B;
pub const ERROR: u32 = 0x00FF_6B5C;
const DIM: u32 = 0x0078_8090;

/// One on-screen notification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast {
    pub text: String,
    pub color: u32,
    /// Frames of life left; 0 means "expired, drop it".
    pub ttl: u32,
}

impl Toast {
    /// Opacity for this toast's remaining life: fully opaque until the last [`FADE_FRAMES`], then a linear
    /// ramp to transparent.
    pub fn alpha(&self) -> u8 {
        if self.ttl >= FADE_FRAMES {
            255
        } else {
            (self.ttl * 255 / FADE_FRAMES) as u8
        }
    }
}

/// Everything the overlay needs to know about the machine, gathered once per present. Passing a snapshot
/// rather than borrowing the world keeps the drawing code pure enough to unit-test.
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub paused: bool,
    /// **How many frames this window has drawn** since its last reset or ROM swap — a tally, not a
    /// position. It counts the run loop's own iterations and the ones a bus client drives, and a
    /// save-state load leaves it alone; the bus's `frame` is derived from the emulated clock and answers a
    /// different question. It therefore renders as `DRAWS n`, never as `F n`, so that a reader cannot join
    /// it to `frameToken` (ledger L-08: relabel the status line, do NOT sync the counter).
    pub draws: u64,
    /// The selected save-state slot.
    pub slot: usize,
    /// Which slots have a file on disk. Re-probed only when it can have changed (a save, a load, a slot
    /// change), never per frame.
    pub occupied: [bool; SLOT_COUNT],
    /// Volume step and mute, as `Some((step, max, muted))`; `None` in a build with no audio feature.
    pub volume: Option<(u8, u8, bool)>,
    /// The console **audio** output-filter revision in use ("VA0-VA2", "RAW", …), for the status line.
    ///
    /// Carried already-shortened and rendered under an `AUDIO` label, because the bare revision name was read
    /// as a *video* setting — a board revision — by the one person it is drawn for (2026-08-29,
    /// `F-HUD-FILTER-LABEL`). Its neighbours here are the aspect mode and the native frame size, so a bare
    /// `MODEL1-VA0-VA2` sitting among them is not merely unlabelled, it is labelled by its company.
    pub filter: Option<&'static str>,
    /// **Whether this window is serving the Aether bus.** A fact about the window that nothing on screen used
    /// to state, in either direction: the owner twice launched without `--aether`, went to a client, and
    /// found it offline with the player silent about why (aurora's ask, 2026-08-28).
    ///
    /// Stated in both directions rather than badged only when off. The house pattern one field down — the
    /// layer badge, which appears only for the abnormal state — is deliberately *not* followed here, because
    /// "hidden layers" is visible in the picture itself while a bus is invisible either way, so an absent
    /// field would leave the reader inferring from nothing. That is the defect this field exists to remove.
    pub aether: bool,
    /// The display aspect mode's short name.
    pub aspect: &'static str,
    /// The native frame size currently being presented.
    pub native: (usize, usize),
    /// **Which display layers are being drawn.** Not a frontend notion of hidden layers — the core's own
    /// [`LayerMask`], the very one `emulator/set_layer_enabled` moves, so the badge cannot say a layer is
    /// hidden that the renderer is still drawing.
    pub layers: LayerMask,
}

/// The overlay's own state: the live toasts and whether the persistent status line is showing.
#[derive(Debug, Default)]
pub struct Overlay {
    toasts: VecDeque<Toast>,
    /// Persistent status line (F3). Off by default — a player wants the picture, not a dashboard.
    pub status_line: bool,
    /// Frames left of a *temporary* status line ([`Overlay::flash`]). Save-state work shows the slot strip
    /// without the user having to know F3 exists, then gets out of the way again.
    status_flash: u32,
    /// Presented frames the machine has been *continuously* paused for, saturating. Reset to 0 the moment
    /// a present sees it running, which is what makes the banner's dwell a dwell and not a delay.
    paused_frames: u32,
}

impl Overlay {
    pub fn new() -> Self {
        Self::default()
    }

    /// Post a notification. Identical to the previous message? Then just refresh its life instead of stacking
    /// a duplicate — holding `=` to ramp the volume would otherwise fill the screen with ten copies of itself.
    pub fn push(&mut self, text: impl Into<String>, color: u32) {
        let text = text.into();
        if let Some(last) = self.toasts.back_mut() {
            if last.text == text {
                last.ttl = TOAST_FRAMES;
                last.color = color;
                return;
            }
        }
        self.toasts.push_back(Toast {
            text,
            color,
            ttl: TOAST_FRAMES,
        });
        while self.toasts.len() > MAX_TOASTS {
            self.toasts.pop_front();
        }
    }

    /// Show the status line for [`TOAST_FRAMES`] frames without latching it on. Used by the save-state
    /// controls: the slot strip is the answer to "which slot is selected, and does it have anything in it",
    /// so it should appear when you touch a slot rather than only when you have found the F3 key.
    pub fn flash(&mut self) {
        self.status_flash = TOAST_FRAMES;
    }

    /// Whether the status line is showing right now — latched by F3 or flashed by a slot action.
    pub fn showing_status(&self) -> bool {
        self.status_line || self.status_flash > 0
    }

    /// Age every toast by one presented frame and retire the expired ones, and advance (or reset) the paused
    /// banner's dwell. Driven from the present, not the emulated frame, so notifications last the same
    /// wall-clock time whether the audio pacer asked for 0, 1 or 2 emulated frames this iteration — and so
    /// they still expire while paused.
    ///
    /// `paused` is the machine's state for the frame *about to be drawn*, which is why the dwell lives here
    /// rather than in [`draw`](Self::draw): the banner's rule is about elapsed presented time, and the
    /// present is the only thing that knows time has passed.
    pub fn tick(&mut self, paused: bool) {
        for t in &mut self.toasts {
            t.ttl = t.ttl.saturating_sub(1);
        }
        // `retain` rather than popping from the front: a duplicate `push` refreshes a toast in place, so the
        // queue is not necessarily ordered by remaining life and the expired one can be anywhere in it.
        self.toasts.retain(|t| t.ttl > 0);
        self.status_flash = self.status_flash.saturating_sub(1);
        // Reset, not decay: one running frame in the middle of a pause means the machine was *not*
        // continuously paused, which is exactly the client-write-burst shape the dwell exists to swallow.
        self.paused_frames = if paused {
            self.paused_frames.saturating_add(1)
        } else {
            0
        };
    }

    /// Has the machine been paused long enough for the banner to be worth showing? See
    /// `PAUSED_BANNER_DWELL_FRAMES` for why the answer is not simply "is it paused".
    fn banner_due(&self) -> bool {
        self.paused_frames >= PAUSED_BANNER_DWELL_FRAMES
    }

    /// The live toasts, oldest first.
    pub fn toasts(&self) -> impl DoubleEndedIterator<Item = &Toast> {
        self.toasts.iter()
    }

    /// Font scale for a window `win_h` pixels tall: one font pixel per emulated scanline-scale step, clamped
    /// so the overlay is legible in a small window and does not become a billboard in a huge one.
    pub fn font_scale(win_h: usize) -> usize {
        (win_h / 224).clamp(1, 4)
    }

    /// Font scale for the **status line** specifically: one step below the rest of the overlay, never below 1.
    ///
    /// **Why it is not [`font_scale`](Self::font_scale).** Both the text and the picture grow with the window,
    /// so the status line's budget in *characters* is very nearly constant — measured across 224px to 896px
    /// windows it was 34 characters at every one of them. The line wants ~51, so it was being cut from the
    /// right with no ellipsis and no complaint, and had been for some time: before the bus and audio fields
    /// existed it still ran to 41 characters, losing the frame counter entirely and cutting the native
    /// resolution mid-number (`320X2`). Nothing announced that, because [`fit`] returns a shorter string
    /// rather than an error.
    ///
    /// Dropping one step is the whole fix. A toast or the `PAUSED` banner is a *message* and wants to be
    /// read across the room; the status line is a **readout** consulted deliberately by someone who pressed
    /// F3, and one step down roughly halves its width cost while staying a crisp 5x7 bitmap at 2x or 3x. At a
    /// 224px window there is no step to drop and the line still truncates — that floor is real and is left
    /// visible rather than papered over.
    pub fn status_font_scale(win_h: usize) -> usize {
        Self::font_scale(win_h).saturating_sub(1).max(1)
    }

    /// Draw the whole overlay into the `w * h` presentation buffer, anchored to `area` — the rectangle the
    /// game's picture occupies. Anything landing outside the buffer is clipped by [`font`], so a picture
    /// larger than the window (or a window smaller than the text) is safe.
    pub fn draw(&self, buf: &mut [u32], w: usize, h: usize, area: Rect, st: &Status) {
        let px = Self::font_scale(area.h.max(1));
        let margin = (2 * px).max(4);
        let mut c = font::Canvas::new(buf, w, h);

        if self.showing_status() {
            // The badge reservation inside `draw_status_line` is still measured at the *overlay's* scale,
            // which is what the badge is drawn at — the status line getting smaller must not let it creep
            // under a badge that did not.
            self.draw_status_line(
                &mut c,
                area,
                st,
                Self::status_font_scale(area.h.max(1)),
                margin,
                px,
            );
        }
        // **Unconditional** — not behind `showing_status`, not behind a lens, not on a timer. The mask is
        // on until someone turns it off, and so is the sentence that says so. See [`Self::layer_badge`].
        self.draw_layer_badge(&mut c, area, st, px);
        // Both halves, and both are load-bearing: `st.paused` is this frame's truth, `banner_due` is the
        // dwell the presented frames have accumulated. A resume clears the counter on the next `tick`, so
        // the two can only disagree for the frames a pause has not yet earned.
        if st.paused && self.banner_due() {
            draw_paused_banner(&mut c, area, px);
        }
        self.draw_toasts(&mut c, area, px, margin);
    }

    /// **What [`draw`](Self::draw) just put on the glass, as text** — the overlay's half of
    /// `emulator/screen_text` (contract §11.29, CR-H).
    ///
    /// Deliberately written directly beneath `draw` and in the same order, because it is a *reading* of that
    /// function and the two must not be able to disagree about what is on screen. Every decision here is
    /// delegated to the same helper the paint uses — `showing_status`, `status_line_layout`,
    /// `visible_toasts` — so this function contains no width, no scale and no fit of its own. Anything it
    /// computed itself would be a second opinion, and a second opinion is how a readout comes to describe a
    /// screen that no longer exists.
    ///
    /// # Which surfaces are here, and which are not
    ///
    /// The **status line** and the **toasts** — the two surfaces whose text has no other reader on the bus.
    /// Not the **layer badge** and not the **PAUSED banner**: the contract's `kind` enum has no value for
    /// either, so there is nowhere on the wire to put them, and inventing one is a contract edit rather than
    /// a handler decision. Both are already answerable structurally — `emulator/get_layer_states` for the
    /// badge, `emulator/status`'s run state for the banner — which is why the enum can reasonably omit them.
    /// Recorded here rather than left as an unexplained gap between what `draw` paints and what this returns.
    ///
    /// # Order
    ///
    /// Back to front, matching `draw`: the status line, then the toasts. Toasts are emitted **oldest
    /// first** — the order they were posted, which is the order a reader wants a message log in — while
    /// `draw_toasts` paints them newest-first from the bottom up. Neither order is the other's Z order:
    /// toasts do not overlap each other.
    pub fn text_surfaces(&self, area: Rect, st: &Status) -> Vec<crate::screen_text::Surface> {
        use crate::screen_text::{Kind, Surface};
        let px = Self::font_scale(area.h.max(1));
        let margin = (2 * px).max(4);
        let mut out = Vec::new();
        if self.showing_status() {
            if let Some(sl) = Self::status_line_layout(
                area,
                st,
                Self::status_font_scale(area.h.max(1)),
                margin,
                px,
            ) {
                out.push(Surface::drawn(
                    Kind::StatusLine,
                    sl.full.clone(),
                    sl.rendered().to_string(),
                ));
            }
        }
        // `visible_toasts` yields newest-first because that is the order the stack is painted in; reversed
        // here so the wire reads oldest-first. The list is already only the toasts that reached the glass.
        let mut toasts: Vec<crate::screen_text::Surface> = self
            .visible_toasts(area, px, margin)
            .into_iter()
            .map(|(_, t, rendered)| {
                Surface::drawn(Kind::Toast, t.text.clone(), rendered.into_owned())
            })
            .collect();
        toasts.reverse();
        out.append(&mut toasts);
        out
    }

    /// **The standing statement that a display layer is hidden**, or `None` when every layer is drawn.
    ///
    /// Text and geometry in one place, so the width [`draw_status_line`](Self::draw_status_line) reserves
    /// and the width [`draw_layer_badge`](Self::draw_layer_badge) paints cannot disagree — the same argument
    /// that made `PAUSED_WORD` a `const`.
    ///
    /// # Why this is a correctness requirement and not decoration
    ///
    /// A mask changes what the picture *is*. With no standing statement that it is on, the person who set
    /// it will forget, and then read a masked picture as the machine's — which is worse than not having the
    /// toggle, because a wrong picture that looks right is indistinguishable from a right one. A toast
    /// cannot carry this: toasts expire, and the mask does not. So this is drawn on **every** frame the mask
    /// is non-default and on none where it is not, and it names the hidden layers rather than merely
    /// admitting to a mask, because "something is hidden" sends you hunting and "planeB is hidden" does not.
    ///
    /// # Where it sits, and why nothing collides with it
    ///
    /// Right-aligned inside the **F3 status band** ([`status_band`]) — the one strip of the picture every
    /// lens already clears unconditionally, so no lens has to learn about this and none can be dimmed by it
    /// (the interference the CPU chip's dodge exists for). The only other tenant of that band is the status
    /// line itself, which grows from the left and is handed a shortened width whenever this is showing, so
    /// the two are exclusive by construction rather than by luck.
    ///
    /// The font scale steps down to 1 before the badge is dropped, and the text is **never truncated**:
    /// `HIDDEN: plan` names a layer that does not exist. A picture with no room for it at scale 1 gets
    /// nothing — the same call [`banner_layout`] makes, for the same reason.
    fn layer_badge(area: Rect, px: usize, mask: LayerMask) -> Option<(String, Rect, usize, usize)> {
        let hidden = mask.hidden();
        if hidden.is_empty() {
            return None;
        }
        let text = format!("HIDDEN: {}", hidden.join(" "));
        let margin = (2 * px).max(4);
        let pad_for = |p: usize| 2 * p;
        let scale = (1..=px).rev().find(|&p| {
            font::text_width(&text) * p + 2 * pad_for(p) + 2 * margin <= area.w
                && font::GLYPH_H * p + 2 * pad_for(p) + 2 * margin <= area.h
        })?;
        let pad = pad_for(scale);
        let w = font::text_width(&text) * scale + 2 * pad;
        let h = font::GLYPH_H * scale + 2 * pad;
        Some((
            text,
            Rect {
                x: area.x + area.w - margin - w,
                y: area.y + margin,
                w,
                h,
            },
            scale,
            pad,
        ))
    }

    /// Paint the badge described by [`layer_badge`](Self::layer_badge). Amber — the colour this overlay
    /// already uses for "a mode is on" (the `PAUSED` banner, an occupied save slot), so it reads as a state
    /// rather than as an error.
    fn draw_layer_badge(&self, c: &mut font::Canvas, area: Rect, st: &Status, px: usize) {
        let Some((text, r, scale, pad)) = Self::layer_badge(area, px, st.layers) else {
            return;
        };
        c.fill_rect(r.x as i32, r.y as i32, r.w, r.h, 0x0000_0000, 210);
        c.text((r.x + pad) as i32, (r.y + pad) as i32, scale, ACCENT, &text);
    }

    /// **What the status line will say, and how much of it survives** — or `None` when the line is not
    /// drawn at all at this geometry.
    ///
    /// Extracted from [`draw_status_line`](Self::draw_status_line) so that the readout
    /// (`emulator/screen_text`) and the paint are **one** computation rather than two that agree today.
    /// A restated copy would agree with itself while drifting from the drawing code, which is the shape
    /// this repo keeps paying for — the same argument that put [`status_text_avail`] in this module.
    ///
    /// Both `None` cases are real and neither is an error: the picture can be too narrow for even the slot
    /// strip, and too short for a line of text to sit inside it. A reader that reported a status line in
    /// either case would be reporting text that is not on the glass.
    fn status_line_layout(
        area: Rect,
        st: &Status,
        px: usize,
        margin: usize,
        badge_px: usize,
    ) -> Option<StatusLine> {
        let pad = 2 * px;
        // Everything is fitted to the picture's width, so the status line can never run into the letterbox.
        // **Minus whatever the layer badge is standing in**, because the two share this band and the badge
        // is the one that cannot be shortened: the status line truncates gracefully (it is a readout), and a
        // truncated `HIDDEN: plan` names a layer that does not exist. So the reservation is one-directional
        // by design, and it is a reservation rather than a redraw order — the badge paints last, and a
        // status line allowed to run under it would be a wrong picture *underneath the sentence saying the
        // picture is wrong*.
        //
        // Measured at `badge_px`, the scale the badge is actually drawn at — which since 2026-08-29 is a step
        // larger than this line's own `px`. Reserving at the smaller scale would reserve less room than the
        // badge occupies, which is the one direction this reservation must never be wrong in.
        let badge_w = Self::layer_badge(area, badge_px, st.layers)
            .map_or(0, |(_, r, _, _)| r.w + 2 * badge_px);
        let avail = area.w.saturating_sub(2 * margin + badge_w);
        // Not even the slot strip fits. The strip is drawn as fixed-width boxes rather than text, so there
        // is nothing to truncate — drop the whole line instead of letting it run off the picture.
        let text_avail = status_text_avail(avail, px)?;
        if area.h < margin + font::GLYPH_H * px + 2 * pad {
            return None; // the picture is too short for a line of text to sit inside it
        }
        let full = status_text(st);
        let fit_len = fit(&full, text_avail, px).len();
        Some(StatusLine {
            full,
            fit_len,
            avail,
        })
    }

    /// The persistent status line (F3): slot strip, volume, filter, aspect, native size, draw tally.
    fn draw_status_line(
        &self,
        c: &mut font::Canvas,
        area: Rect,
        st: &Status,
        px: usize,
        margin: usize,
        badge_px: usize,
    ) {
        let Some(sl) = Self::status_line_layout(area, st, px, margin, badge_px) else {
            return;
        };
        let line = sl.rendered();
        let pad = 2 * px;
        let strip_w = slot_strip_width(px);
        // Every width below comes from the layout above — including `sl.avail`, which already has the badge's
        // reservation taken out of it. Nothing here re-derives a budget.
        let panel_w = (strip_w + pad + font::text_width(line) * px + 2 * pad).min(sl.avail.max(1));
        let panel_h = font::GLYPH_H * px + 2 * pad;
        let ox = (area.x + margin) as i32;
        let oy = (area.y + margin) as i32;
        c.fill_rect(ox, oy, panel_w, panel_h, 0x0000_0000, font::PANEL_ALPHA);
        let x = ox + pad as i32;
        let y = oy + pad as i32;
        draw_slot_strip(c, x, y, px, st.slot, &st.occupied);
        c.text(x + (strip_w + pad) as i32, y, px, INFO, line);
    }

    /// **The toasts that actually reach the glass**, newest first — the order [`draw_toasts`] paints them —
    /// each with its row index in the stack and the string that is actually painted — the whole text, or
    /// what [`fit_marked`] kept of it plus its mark.
    ///
    /// Extracted for the same reason [`status_line_layout`](Self::status_line_layout) was: the readout and
    /// the paint must be one computation. Both exclusions are load-bearing and neither is an error — the
    /// stack can run off the top of the picture, and a picture can be too narrow for even one glyph — and a
    /// reader that reported those toasts would be reporting text that is not on screen.
    ///
    /// The row index is carried rather than re-derived because it is **not** the position in this list: a
    /// toast whose text fits no glyph is skipped but still consumes its slot in the stack, so re-numbering
    /// the survivors would slide every toast above it down one row.
    fn visible_toasts(
        &self,
        area: Rect,
        px: usize,
        margin: usize,
    ) -> Vec<(usize, &Toast, Cow<'_, str>)> {
        let pad = 2 * px;
        let row_h = font::LINE_H * px + 2 * pad;
        let bottom = (area.y + area.h) as i32 - margin as i32;
        let avail = Self::toast_text_avail(area, px, margin);
        let mut out = Vec::new();
        for (i, t) in self.toasts().rev().enumerate() {
            let y = bottom - ((i + 1) * row_h) as i32;
            if y < area.y as i32 {
                break; // the stack has reached the top of the picture — never spill into the letterbox
            }
            let rendered = fit_marked(&t.text, avail, px);
            if rendered.is_empty() {
                continue; // the picture is too narrow for even one glyph — draw no bare panel either
            }
            out.push((i, t, rendered));
        }
        out
    }

    /// Device pixels of ink a toast's text may occupy in `area`: the picture's width less the outer margin
    /// and the panel's padding on both sides. `pad` is `2 * px`, the same figure [`draw_toasts`] paints with.
    ///
    /// Extracted like [`status_text_avail`](Self::status_text_avail) so a test can stand at the real toast
    /// width instead of a width it picked for itself.
    pub fn toast_text_avail(area: Rect, px: usize, margin: usize) -> usize {
        let pad = 2 * px;
        area.w.saturating_sub(2 * margin + 2 * pad)
    }

    /// Toasts, stacked upward from the bottom-left corner with the newest at the bottom.
    fn draw_toasts(&self, c: &mut font::Canvas, area: Rect, px: usize, margin: usize) {
        let pad = 2 * px;
        let row_h = font::LINE_H * px + 2 * pad;
        let left = (area.x + margin) as i32;
        let bottom = (area.y + area.h) as i32 - margin as i32;
        for (i, t, rendered) in self.visible_toasts(area, px, margin) {
            let y = bottom - ((i + 1) * row_h) as i32;
            let alpha = t.alpha();
            let text = rendered.as_ref();
            let panel_w = font::text_width(text) * px + 2 * pad;
            c.fill_rect(
                left,
                y,
                panel_w,
                font::GLYPH_H * px + 2 * pad,
                0x0000_0000,
                scale_alpha(font::PANEL_ALPHA, alpha),
            );
            c.text(
                left + pad as i32,
                y + pad as i32,
                px,
                fade(t.color, alpha),
                text,
            );
        }
    }
}

/// The status line as it will be painted: what it says, and how much of it fits.
///
/// `fit_len` is a **byte** length, taken from [`fit`]'s own return rather than recomputed, so
/// `full[..fit_len]` is guaranteed to land on a character boundary.
struct StatusLine {
    /// The source string [`status_text`] composed.
    full: String,
    /// Bytes of [`full`](StatusLine::full) that survive [`fit`] at this geometry.
    fit_len: usize,
    /// Device pixels the line's panel may occupy, badge reservation already deducted. Only the paint uses
    /// it; it is carried here so `draw_status_line` does not re-derive a budget this function already knew.
    avail: usize,
}

impl StatusLine {
    /// What is actually on the glass — a prefix of [`full`](StatusLine::full).
    fn rendered(&self) -> &str {
        &self.full[..self.fit_len]
    }
}

/// The longest prefix of `text` whose ink fits in `avail` device pixels at font scale `px`. Truncation, not
/// wrapping: an overlay that reflows would move the picture's own content around, and a status line that ran
/// past the picture into the letterbox is exactly the untidiness anchoring to the picture was meant to avoid.
/// Returns `""` when not even one glyph fits.
pub fn fit(text: &str, avail: usize, px: usize) -> &str {
    let px = px.max(1);
    let mut end = 0;
    let mut ink = 0;
    for (i, c) in text.char_indices() {
        // The first glyph costs its 5 columns; each later one costs the inter-glyph gap as well.
        let cost = if end == 0 { 5 * px } else { font::ADVANCE * px };
        if ink + cost > avail {
            break;
        }
        ink += cost;
        end = i + c.len_utf8();
    }
    &text[..end]
}

/// The one glyph [`fit_marked`] appends when it has to cut a string: U+2026 HORIZONTAL ELLIPSIS, which
/// `font.rs` draws as three dots on the baseline.
pub const TRUNCATION_MARK: char = '\u{2026}';

/// `text` whole when its ink fits in `avail` device pixels at font scale `px`; otherwise the longest prefix
/// that fits **together with a trailing [`TRUNCATION_MARK`]**, so a cut is visible on the glass instead of
/// the message simply ending early. Returns `""` when not even one glyph of `text` plus the mark fits — a
/// bare `…` would say "there was a message" and nothing else, and the caller draws no panel for `""`.
///
/// This is what toasts are fitted with (F-TOAST-TRUNCATES). A toast that was cut silently lost whatever
/// was on its right — for `open ROM: cannot read <dir> (<reason>)` that was the reason, the one part that
/// answered the question — and read as a complete sentence, so nobody knew to widen the window or look
/// elsewhere. The status line keeps plain [`fit`]: its fields are ordered so the cut side is the least
/// informative, and it is a fixed-width readout, not a sentence.
///
/// Cost arithmetic: `fit` charges the first glyph 5 columns and each later one [`font::ADVANCE`], so a
/// prefix of `n` glyphs plus the mark costs exactly what `n + 1` glyphs of `text` would. Fitting the prefix
/// into `avail - ADVANCE * px` is therefore the same as fitting prefix-plus-mark into `avail`.
pub fn fit_marked(text: &str, avail: usize, px: usize) -> Cow<'_, str> {
    let px = px.max(1);
    let whole = fit(text, avail, px);
    if whole.len() == text.len() {
        return Cow::Borrowed(text);
    }
    let head = fit(text, avail.saturating_sub(font::ADVANCE * px), px);
    if head.is_empty() {
        return Cow::Borrowed("");
    }
    Cow::Owned(format!("{head}{TRUNCATION_MARK}"))
}

/// Multiply an opacity by a fade factor (both `0..=255`).
fn scale_alpha(base: u8, fade: u8) -> u8 {
    ((u32::from(base) * u32::from(fade)) / 255) as u8
}

/// Scale a packed RGB colour toward black by `alpha`, which is how a fading toast dims (the buffer is opaque,
/// so there is no real alpha channel to use).
fn fade(color: u32, alpha: u8) -> u32 {
    let a = u32::from(alpha);
    let mut out = 0;
    for shift in [16, 8, 0] {
        out |= (((color >> shift) & 0xFF) * a / 255) << shift;
    }
    out
}

/// The status line's text (everything except the slot strip, which is drawn as boxes).
///
/// **Field order is truncation order.** [`fit`] cuts this from the right and says nothing about what it
/// removed, so the sequence below is a priority list rather than a layout: the two fields that answer *"is
/// this window lying to me"* — whether the bus is up, and which output stage is colouring the sound — come
/// before the three that merely describe the picture (aspect, native size, draw tally), because those
/// three are re-derivable by looking at the window and the first two are not.
///
/// The tally's label is `DRAWS`, chosen so that it cannot be read as the bus's `frame` (ledger L-08): it
/// counts what this window has drawn, and says nothing about where the machine is.
pub fn status_text(st: &Status) -> String {
    let mut s = String::new();
    if let Some((v, max, muted)) = st.volume {
        if muted {
            s.push_str("MUTE ");
        } else {
            s.push_str(&format!("VOL {v}/{max} "));
        }
    }
    s.push_str(if st.aether {
        "AETHER ON "
    } else {
        "AETHER OFF "
    });
    if let Some(f) = st.filter {
        s.push_str(&format!("AUDIO {f} "));
    }
    s.push_str(&format!(
        "{} {}X{} DRAWS {}",
        st.aspect, st.native.0, st.native.1, st.draws
    ));
    s
}

/// How many device pixels of the status band are left for [`status_text`] once the fixed-width slot strip and
/// the panel's padding have taken theirs, or `None` when not even the strip fits.
///
/// Extracted from [`Overlay::draw_status_line`] so that a test can ask *how much text actually survives at a
/// real window size* without restating the arithmetic — a restated copy would agree with itself while
/// drifting from the drawing code, which is the shape this repo keeps paying for. `avail` is the band width
/// already reduced by the margins and by any layer badge.
pub fn status_text_avail(avail: usize, px: usize) -> Option<usize> {
    let pad = 2 * px;
    let strip_w = slot_strip_width(px);
    (avail >= strip_w + 4 * pad).then(|| avail.saturating_sub(strip_w + pad + 2 * pad))
}

/// Width in device pixels of the ten-slot strip at font scale `px`: one `ADVANCE`-wide cell per slot plus a
/// one-pixel-scaled box around the selected one on each side.
pub fn slot_strip_width(px: usize) -> usize {
    SLOT_COUNT * (font::ADVANCE + 2) * px
}

/// Draw the save-state slot strip: ten digits, the **selected** one boxed, **occupied** ones in amber and
/// empty ones dim. That is the whole state the keyboard-only slot controls used to keep to themselves.
pub fn draw_slot_strip(
    c: &mut font::Canvas,
    x: i32,
    y: i32,
    px: usize,
    slot: usize,
    occupied: &[bool; SLOT_COUNT],
) {
    let cell = (font::ADVANCE + 2) * px;
    for (i, &full) in occupied.iter().enumerate() {
        let cx = x + (i * cell) as i32;
        if i == slot {
            // A filled box behind the selected slot, and its digit knocked out in black.
            c.fill_rect(
                cx,
                y - px as i32,
                cell,
                (font::GLYPH_H + 2) * px,
                if full { ACCENT } else { INFO },
                255,
            );
        }
        let color = match (i == slot, full) {
            (true, _) => 0x0000_0000,
            (false, true) => ACCENT,
            (false, false) => DIM,
        };
        let digit = char::from(b'0' + i as u8).to_string();
        c.text(cx + px as i32, y, px, color, &digit);
    }
}

/// The word the banner shows. A `const` because [`paused_banner_rect`] measures it and
/// [`draw_paused_banner`] draws it: two copies of `"PAUSED"` would be two things that could
/// disagree about how wide the banner is, and the whole point of the accessor is that they cannot.
const PAUSED_WORD: &str = "PAUSED";

/// The banner's geometry: where it lands, at what font scale, with what padding — or `None` when
/// the picture cannot hold it.
///
/// Split out of the draw so that a *reader* can ask where it is. See [`paused_banner_rect`].
fn banner_layout(area: Rect, px: usize) -> Option<(Rect, usize, usize)> {
    // A twelfth of the way down the picture, so it clears the game's own top-of-screen HUD.
    let y_off = area.h / 12;
    let head_room = area.h - y_off;
    // Twice the overlay scale — the one thing on screen that should be unmissable — stepped down until the
    // whole word *and* its panel fit inside the picture on both axes. The word is never truncated: "PAU" is
    // not a pause indicator. A picture with no room for it at 1x gets nothing rather than a leak into the
    // letterbox, which at that size is a handful of pixels nobody could read anyway.
    let pad_for = |p: usize| 3 * p;
    let px = (1..=px * 2).rev().find(|&p| {
        font::text_width(PAUSED_WORD) * p + 2 * pad_for(p) <= area.w
            && font::GLYPH_H * p + 2 * pad_for(p) <= head_room
    })?;
    let pad = pad_for(px);
    let w = font::text_width(PAUSED_WORD) * px + 2 * pad;
    let h = font::GLYPH_H * px + 2 * pad;
    Some((
        Rect {
            x: area.x + area.w.saturating_sub(w) / 2,
            y: area.y + y_off,
            w,
            h,
        },
        px,
        pad,
    ))
}

/// **Where the `PAUSED` banner lands**, in the same window coordinates a lens draws in, or `None`
/// when the picture is too small for it to appear at all.
///
/// Exported because the overlay is drawn *after* the lenses (main.rs:1776-1817) and its panels are
/// only `PANEL_ALPHA` opaque: a white lens glyph underneath one drops to about 65/255 and reads as
/// **absent**, while its neighbours stay bright. That is not occlusion, it is interference — the
/// same argument that put the sprite outlines beneath the lens panels, one layer up — and a
/// register that renders `D0 D00_0000` is a plausible wrong 32-bit value rather than a visibly
/// missing one.
///
/// The CPU chip is the caller: it auto-shows whenever the machine stops, so it and the banner are
/// on screen together **by design** every time the user hits Space, and it uses this to step out of
/// the way. Exposing the band rather than moving the banner keeps the banner where it is meant to
/// be — centred, unmissable, clear of the game's HUD — and puts the avoidance in the thing that has
/// somewhere else to go.
pub fn paused_banner_rect(area: Rect, px: usize) -> Option<Rect> {
    banner_layout(area, px).map(|(r, _, _)| r)
}

/// **The vertical space the F3 status line reserves** at the top of the picture, in device pixels:
/// a whole line box plus the panel's padding, measured at the *overlay's* font scale.
///
/// A whole `LINE_H` rather than the `GLYPH_H` the panel actually stands, so a lens clearing it has
/// the font's own leading to spare. The status line latches and flashes on its own schedule, so
/// callers offset by this **unconditionally** — a panel that jumped a row whenever a save slot
/// flashed would be worse than one sitting a row lower than it strictly needs to.
///
/// Unlike the banner there is no width to report: the status line spans from the left margin and
/// its width depends on what it currently says, so the only safe answer for a lens is "start below
/// it".
pub fn status_row_height(px: usize) -> usize {
    font::LINE_H * px + 2 * (2 * px)
}

/// **Where the F3 status line lands**, as a band spanning the picture's full width.
///
/// Full width on purpose, and wider than the panel actually painted: the status line's width
/// depends on what it currently says (slot strip, volume, filter, aspect, native size, frame), so a
/// lens asking "may I draw here?" cannot be told a truthful column range that will still be true a
/// frame later. The honest answer is a band, and a lens either clears it or does not draw.
///
/// The `y` is the overlay's own margin — **not** the caller's, which is the trap this replaces: the
/// CPU chip computed `area.y + margin + status_row_height(px)` with *its* margin, which is smaller
/// than the overlay's whenever the register block drops a font scale, so its panel overlapped the
/// band by two rows and only the panel's padding kept a glyph out of it.
///
/// **Callers pass the overlay's `px`, and since 2026-08-29 the status line itself draws one step
/// smaller ([`Overlay::status_font_scale`]) — so this band is now taller than the panel it guards.**
/// Deliberately left that way: an over-reservation costs a lens a few rows it did not have to yield,
/// while an under-reservation puts a callout through the status line, and only one of those is a
/// wrong picture. Do not "correct" it to the smaller scale without re-checking every caller.
pub fn status_band(area: Rect, px: usize) -> Rect {
    Rect {
        x: area.x,
        y: area.y + (2 * px).max(4),
        w: area.w,
        h: status_row_height(px),
    }
}

/// The centered `PAUSED` banner. Since a paused frontend re-presents the retained framebuffer forever, this
/// is the only thing that tells a paused emulator apart from a hung one.
fn draw_paused_banner(c: &mut font::Canvas, area: Rect, px: usize) {
    let Some((r, px, pad)) = banner_layout(area, px) else {
        return;
    };
    c.fill_rect(r.x as i32, r.y as i32, r.w, r.h, 0x0000_0000, 210);
    c.text(
        (r.x + pad) as i32,
        (r.y + pad) as i32,
        px,
        ACCENT,
        PAUSED_WORD,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The picture filling the whole `w x h` buffer — the common case, and the one most assertions want.
    fn whole(w: usize, h: usize) -> Rect {
        Rect { x: 0, y: 0, w, h }
    }

    /// A background that is **not** zero. The banner's panel is black at alpha 210 and the overlay's other
    /// panels are translucent too, so over a zeroed buffer a panel is indistinguishable from bare
    /// background and an "it drew something" assertion passes for a draw that drew nothing.
    const GROUND: u32 = 0x0012_3456;

    /// Hold the machine in one pause state for `n` presented frames. The frontend calls `tick` once per
    /// present (`main.rs`), immediately before `draw`, so this is that loop.
    fn present_frames(o: &mut Overlay, paused: bool, n: u32) {
        for _ in 0..n {
            o.tick(paused);
        }
    }

    /// Pixels inside the banner's rectangle that differ from `GROUND`. The banner is the only thing that
    /// draws there — the status line is a margin above it and the toasts are at the bottom — so this is
    /// the banner's ink, counted rather than merely detected.
    fn banner_ink(buf: &[u32], w: usize, area: Rect) -> usize {
        let px = Overlay::font_scale(area.h.max(1));
        let Some(r) = paused_banner_rect(area, px) else {
            return 0;
        };
        (r.y..r.y + r.h)
            .flat_map(|y| (r.x..r.x + r.w).map(move |x| (x, y)))
            .filter(|&(x, y)| buf[y * w + x] != GROUND)
            .count()
    }

    /// Draw one frame over a `GROUND` field and report the banner's ink.
    fn ink_after(o: &Overlay, w: usize, h: usize, st: &Status) -> usize {
        let mut buf = vec![GROUND; w * h];
        o.draw(&mut buf, w, h, whole(w, h), st);
        banner_ink(&buf, w, whole(w, h))
    }

    fn status() -> Status {
        Status {
            paused: false,
            draws: 1234,
            slot: 3,
            occupied: [false; SLOT_COUNT],
            volume: Some((7, 10, false)),
            filter: Some("VA0-VA2"),
            aether: false,
            aspect: "4:3",
            layers: LayerMask::ALL,
            native: (320, 224),
        }
    }

    /// A mask hiding `layers`, built through the core's own setter so the test cannot invent a layer.
    fn masking(layers: &[&str]) -> LayerMask {
        let mut m = LayerMask::ALL;
        for name in layers {
            let (_, layer) = LayerMask::targets()
                .into_iter()
                .find(|(n, _)| n == name)
                .unwrap_or_else(|| panic!("{name} is not a mask target"));
            assert!(m.set(layer, false), "{name} refused the mask");
        }
        m
    }

    /// Pixels inside the layer badge's rectangle that differ from `GROUND`.
    fn badge_ink(buf: &[u32], w: usize, area: Rect, mask: LayerMask) -> usize {
        let px = Overlay::font_scale(area.h.max(1));
        let Some((_, r, _, _)) = Overlay::layer_badge(area, px, mask) else {
            return 0;
        };
        (r.y..r.y + r.h)
            .flat_map(|y| (r.x..r.x + r.w).map(move |x| (x, y)))
            .filter(|&(x, y)| buf[y * w + x] != GROUND)
            .count()
    }

    /// **The standing statement is drawn on every frame a layer is hidden, and on none where it is not.**
    ///
    /// This is the correctness claim, not a decoration check: a mask changes what the picture *is*, so a
    /// picture with a mask and no statement will be read as the machine's. Toasts expire and the mask does
    /// not, which is why this is asserted **after** every toast has aged out and with the F3 status line
    /// off — the two states in which a "the user was told" argument is otherwise pure assertion.
    ///
    /// Planting the defect: make `Overlay::draw`'s `draw_layer_badge` call conditional on
    /// `self.showing_status()` (the obvious "put it in the status line" implementation) and the masked case
    /// below fails with *"a hidden layer must be stated on screen even with the status line off and every
    /// toast expired"*. Verified.
    #[test]
    fn a_hidden_layer_is_stated_on_screen_for_as_long_as_it_is_hidden() {
        let (w, h) = (960, 672);
        let mut o = Overlay::new();
        o.push("SOMETHING TRANSIENT", INFO);
        // Age well past a toast's whole life, so nothing on screen is left from the toggle itself.
        present_frames(&mut o, false, TOAST_FRAMES + 60);
        assert_eq!(o.toasts().count(), 0, "the transient half must be gone");
        assert!(!o.showing_status(), "and F3 must be off");

        let mut st = status();
        let mut buf = vec![GROUND; w * h];
        o.draw(&mut buf, w, h, whole(w, h), &st);
        assert_eq!(
            badge_ink(&buf, w, whole(w, h), masking(&["planeA"])),
            0,
            "nothing is hidden, so nothing may be claimed — the badge must not be a permanent fixture"
        );

        st.layers = masking(&["planeA"]);
        let mut buf = vec![GROUND; w * h];
        o.draw(&mut buf, w, h, whole(w, h), &st);
        assert!(
            badge_ink(&buf, w, whole(w, h), st.layers) > 0,
            "a hidden layer must be stated on screen even with the status line off and every toast \
             expired"
        );
    }

    /// The badge **names the hidden layers**, in the wire's own words, and is never truncated.
    ///
    /// "A mask is set" sends the reader hunting; `HIDDEN: planeB` does not. And a truncated `HIDDEN: plan`
    /// names a layer that does not exist — the `PAUSED_WORD` rule ("PAU is not a pause indicator") applied
    /// to a longer string, which is why the scale steps down instead of the text being cut.
    #[test]
    fn the_badge_names_every_hidden_layer_and_never_truncates() {
        let area = whole(960, 672);
        let px = Overlay::font_scale(area.h);
        for hidden in [
            vec!["planeA"],
            vec!["planeB", "sprites"],
            LayerMask::targets().iter().map(|(n, _)| *n).collect(),
        ] {
            let mask = masking(&hidden);
            let (text, r, scale, pad) =
                Overlay::layer_badge(area, px, mask).expect("a masked badge must have a form");
            for name in &hidden {
                assert!(text.contains(name), "{text:?} does not name {name}");
            }
            // Not a substring check on the joined list — the width is what a truncation would break.
            assert_eq!(
                r.w,
                font::text_width(&text) * scale + 2 * pad,
                "the rect must hold the WHOLE text: {text:?}"
            );
            assert!(
                r.x + r.w <= area.x + area.w && r.y + r.h <= area.y + area.h,
                "the badge escaped the picture: {r:?} in {area:?}"
            );
        }
        assert!(
            Overlay::layer_badge(area, px, LayerMask::ALL).is_none(),
            "an all-on mask has nothing to state"
        );
    }

    /// **The badge and the F3 status line share the status band and must not overlap.** They are the
    /// band's only two tenants (every lens clears it unconditionally), the status line grows from the left
    /// and the badge is right-aligned, and the status line is handed a shortened width whenever the badge
    /// is showing — so this is a reservation, checked in pixels, not a redraw order.
    ///
    /// Planting the defect: drop the `badge_w` subtraction in `draw_status_line` and the widest case below
    /// fails with *"the status line ran under the badge"* — the status line's ink reaching into the
    /// badge's columns, which on screen is a wrong readout painted underneath the sentence saying the
    /// picture is wrong. Verified at 320x224, where the two are closest.
    #[test]
    fn the_status_line_never_runs_under_the_badge() {
        for (w, h) in [(320usize, 224usize), (640, 448), (960, 672)] {
            let area = whole(w, h);
            let px = Overlay::font_scale(h);
            let mut o = Overlay::new();
            o.status_line = true;
            let mut st = status();
            st.layers = masking(
                &LayerMask::targets()
                    .iter()
                    .map(|(n, _)| *n)
                    .collect::<Vec<_>>(),
            );
            let Some((_, badge, _, _)) = Overlay::layer_badge(area, px, st.layers) else {
                // Loud rather than a silent skip: if the badge cannot form at this size the row proves
                // nothing, and a quietly-passing row is exactly what the poison bar warns about.
                panic!("COULD NOT MEASURE at {w}x{h}: the badge has no form, so no overlap is testable");
            };
            let mut buf = vec![GROUND; w * h];
            // Draw the status line alone, over a ground, and look for its ink in the badge's columns.
            // Drawing the badge too would paint those columns itself and hide the very thing under test.
            let mut c = font::Canvas::new(&mut buf, w, h);
            // The two scales the live `draw` uses, not one scale for both: the status line is drawn a step
            // smaller than the badge it must stay clear of, and passing `px` for both would test a
            // relationship that no longer exists.
            o.draw_status_line(
                &mut c,
                area,
                &st,
                Overlay::status_font_scale(h),
                (2 * px).max(4),
                px,
            );
            let intruders = (badge.y..badge.y + badge.h)
                .flat_map(|y| (badge.x..badge.x + badge.w).map(move |x| (x, y)))
                .filter(|&(x, y)| buf[y * w + x] != GROUND)
                .count();
            assert_eq!(
                intruders, 0,
                "{w}x{h}: the status line ran under the badge ({intruders} pixels in {badge:?})"
            );
        }
    }

    /// A toast lives for `TOAST_FRAMES` presented frames and then retires itself.
    #[test]
    fn a_toast_expires_after_its_lifetime() {
        let mut o = Overlay::new();
        o.push("SAVED SLOT 3", INFO);
        assert_eq!(o.toasts().count(), 1);
        for _ in 0..TOAST_FRAMES - 1 {
            o.tick(false);
        }
        assert_eq!(o.toasts().count(), 1, "still up one frame before expiry");
        o.tick(false);
        assert_eq!(o.toasts().count(), 0, "and gone on the frame it expires");
    }

    /// Repeating the same message refreshes it rather than stacking duplicates — holding `=` to ramp the
    /// volume must not paper the screen with ten identical lines.
    #[test]
    fn an_identical_message_refreshes_instead_of_stacking() {
        let mut o = Overlay::new();
        o.push("VOLUME 5/10", INFO);
        for _ in 0..100 {
            o.tick(false);
        }
        o.push("VOLUME 5/10", INFO);
        assert_eq!(o.toasts().count(), 1, "no duplicate");
        assert_eq!(
            o.toasts().next().unwrap().ttl,
            TOAST_FRAMES,
            "its life was refreshed"
        );
        // A *different* message does stack.
        o.push("VOLUME 6/10", INFO);
        assert_eq!(o.toasts().count(), 2);
    }

    /// A burst drops the oldest, keeping the newest `MAX_TOASTS` — the most recent thing that happened is
    /// always the one on screen.
    #[test]
    fn a_burst_keeps_the_newest_messages() {
        let mut o = Overlay::new();
        for i in 0..MAX_TOASTS + 4 {
            o.push(format!("MSG {i}"), INFO);
        }
        assert_eq!(o.toasts().count(), MAX_TOASTS);
        let texts: Vec<_> = o.toasts().map(|t| t.text.clone()).collect();
        assert_eq!(texts.first().unwrap(), &format!("MSG {}", 4));
        assert_eq!(
            texts.last().unwrap(),
            &format!("MSG {}", MAX_TOASTS + 3),
            "the newest survives"
        );
    }

    /// Toasts fade out over their last frames instead of blinking off.
    #[test]
    fn a_toast_fades_over_its_last_frames() {
        let mut o = Overlay::new();
        o.push("HELLO", INFO);
        assert_eq!(
            o.toasts().next().unwrap().alpha(),
            255,
            "opaque while fresh"
        );
        for _ in 0..TOAST_FRAMES - FADE_FRAMES + 1 {
            o.tick(false);
        }
        let a = o.toasts().next().unwrap().alpha();
        assert!(a < 255, "it has started fading, got {a}");
        for _ in 0..FADE_FRAMES - 3 {
            o.tick(false);
        }
        assert!(o.toasts().next().unwrap().alpha() < 40, "nearly gone");
    }

    /// The status text names every piece of state the keyboard controls steer, and the mute case replaces the
    /// level rather than showing a stale number next to "MUTE".
    #[test]
    fn the_status_text_reports_the_steerable_state() {
        let s = status_text(&status());
        for want in ["VOL 7/10", "VA0-VA2", "4:3", "320X224", "DRAWS 1234"] {
            assert!(s.contains(want), "status line {s:?} is missing {want:?}");
        }
        let mut muted = status();
        muted.volume = Some((7, 10, true));
        let m = status_text(&muted);
        assert!(m.contains("MUTE"));
        assert!(!m.contains("7/10"), "muted hides the level: {m:?}");
        // A no-audio build simply omits the volume section instead of printing a placeholder.
        let mut silent = status();
        silent.volume = None;
        silent.filter = None;
        let q = status_text(&silent);
        assert!(!q.contains("VOL") && !q.contains("MUTE"));
    }

    /// **The draw tally is labelled as a tally, and nothing on the line can be read as a frame position.**
    ///
    /// Ledger L-08 (`docs/2026-08-22-unadjudicated-decision-ledger.md`) ruled the old `F1234` a RELABEL,
    /// not a sync: the number is what this window has drawn since its last reset or ROM swap — local runs
    /// and bus-driven ones alike, carried unchanged across a save-state load — and never the bus's
    /// clock-derived `frame`, so a reader joining the two was reading a machine coordinate off a liveness
    /// signal. The whole string is asserted because the surface is fixed-width: a fragment check would stay
    /// green if the old label survived somewhere else on the line.
    #[test]
    fn the_draw_tally_is_labelled_as_a_tally_and_never_as_a_frame() {
        let s = status_text(&status());
        assert_eq!(
            s, "VOL 7/10 AETHER OFF AUDIO VA0-VA2 4:3 320X224 DRAWS 1234",
            "the fixture's fields, in truncation order, ending in the draw tally"
        );
        // The control: no word on the line is the old `F<digits>` spelling, and none says FRAME.
        let frame_like: Vec<&str> = s
            .split_whitespace()
            .filter(|w| {
                w.eq_ignore_ascii_case("frame")
                    || (w.len() > 1
                        && w.starts_with('F')
                        && w[1..].chars().all(|c| c.is_ascii_digit()))
            })
            .collect();
        assert!(
            frame_like.is_empty(),
            "these words read as a frame position: {frame_like:?} in {s:?}"
        );
    }

    /// **The audio revision never appears without the word that says it is audio.** The bare `MODEL1-VA0-VA2`
    /// this replaced sat between the volume and the aspect mode, and was read as a board revision by the one
    /// person the line is drawn for. The control below is the half that keeps this from passing vacuously:
    /// deleting the label leaves `VA0-VA2` in the string, so asserting on the revision alone would stay green.
    #[test]
    fn the_audio_revision_is_never_printed_bare() {
        let s = status_text(&status());
        assert!(
            s.contains("AUDIO VA0-VA2"),
            "the revision must carry its label: {s:?}"
        );
        // The control: the revision must not occur anywhere the label does not immediately precede it.
        assert_eq!(
            s.matches("VA0-VA2").count(),
            s.matches("AUDIO VA0-VA2").count(),
            "every occurrence of the revision is a labelled one: {s:?}"
        );
        // And the label is not left stranded on a build with no audio at all.
        let mut silent = status();
        silent.filter = None;
        assert!(!status_text(&silent).contains("AUDIO"));
    }

    /// **The bus state is stated in both directions**, and the two readings are different strings — the whole
    /// point being that a reader never has to infer the state from an absence.
    #[test]
    fn the_status_line_states_the_bus_either_way() {
        let mut off = status();
        off.aether = false;
        let mut on = status();
        on.aether = true;
        let (o, n) = (status_text(&off), status_text(&on));
        assert!(o.contains("AETHER OFF"), "{o:?}");
        assert!(n.contains("AETHER ON"), "{n:?}");
        assert_ne!(o, n);
        // "AETHER ON" is a substring of nothing else here, but "AETHER OFF" contains no "AETHER ON" either —
        // pin it, because a reader scanning for the ON spelling must not match the OFF line.
        assert!(!o.contains("AETHER ON"), "the two readings must not alias");
    }

    /// **Field order is truncation order, and this is what holds it there.** [`fit`] cuts from the right with
    /// no ellipsis and no complaint, so "the status line says AETHER OFF" is a claim about *width*, not about
    /// [`status_text`] alone. Move the bus or audio blocks after the picture fields and this fails.
    #[test]
    fn the_honesty_fields_outlive_the_picture_fields_when_the_line_is_cut() {
        let st = status();
        let full = status_text(&st);
        let px = 1;
        let widest = font::text_width(&full) * px + 8;
        let mut ever_cut_the_tally_while_keeping_the_bus = false;
        for avail in 0..=widest {
            let line = fit(&full, avail, px);
            if line.contains("DRAWS 1234") {
                assert!(
                    line.contains("AETHER OFF") && line.contains("AUDIO VA0-VA2"),
                    "at {avail}px the draw tally survived but an honesty field did not: {line:?}"
                );
            }
            if line.contains("AETHER OFF") && !line.contains("DRAWS 1234") {
                ever_cut_the_tally_while_keeping_the_bus = true;
            }
        }
        // Without this the test above is vacuous: it would also pass if the line never truncated at all, or
        // if every field always appeared together. This is the case the ordering exists to produce.
        assert!(
            ever_cut_the_tally_while_keeping_the_bus,
            "no width drops the draw tally while keeping the bus state — the ordering buys nothing"
        );
    }

    /// What the status line renders to at the **real** geometry the player runs at.
    ///
    /// This is the test the two above could not be: they pin `status_text`, and `status_text` is not what
    /// reaches the glass — [`fit`] is, and it silently returns a shorter string. Adding the bus field made
    /// the line 51 characters against a budget that was 34 at *every* window size, because text and picture
    /// scale together; that regression was invisible to a test asserting only on the fields it added, and it
    /// was measured rather than reasoned about. Dropping the status line one font step
    /// ([`Overlay::status_font_scale`]) is what buys the room back.
    ///
    /// Every width is derived from [`status_text_avail`] and the scale functions, so this cannot drift from
    /// the drawing code by restating its arithmetic.
    #[test]
    fn the_whole_status_line_survives_at_the_sizes_the_player_actually_uses() {
        let st = status();
        let full = status_text(&st);
        let rendered = |win_h: usize| -> String {
            let px = Overlay::status_font_scale(win_h);
            let margin = (2 * Overlay::font_scale(win_h)).max(4);
            let area_w = win_h * 4 / 3; // a 4:3 picture, the aspect the player defaults to
            let avail = area_w.saturating_sub(2 * margin);
            let text_avail = status_text_avail(avail, px)
                .unwrap_or_else(|| panic!("the slot strip should fit at {win_h}"));
            fit(&full, text_avail, px).to_string()
        };
        // A 2x window and everything above it: the entire line, draw tally included. Note 448 — before
        // the status line dropped a font step even a 4x window lost the tally and cut the resolution to
        // `320X2`.
        for win_h in [448usize, 672, 1080, 1440] {
            assert_eq!(
                rendered(win_h),
                full,
                "a {win_h}px-tall picture should show the whole status line"
            );
        }
        // **896 is a dip, not a slope, and it is stated rather than asserted away.** `status_font_scale`
        // steps from 2 to 3 at exactly 896 while the picture grows only 4:3 — so the budget in *glyphs*
        // falls from 59 (at 672) to 51 there before climbing again, and 51 is one glyph short of the
        // 56-glyph line the `DRAWS` label produces. The five cut glyphs are the tally's, which is what the
        // field order exists to arrange; both honesty fields survive.
        //
        // The row this replaces claimed the whole line survived here, and that claim was true only of a
        // FOUR-digit fixture: the control below shows the old `F` label breaking the same row at six
        // digits, which every session reaches in about seventeen minutes. It was a property of the test
        // data, not of the player.
        let dip = rendered(896);
        assert_eq!(
            dip, "VOL 7/10 AETHER OFF AUDIO VA0-VA2 4:3 320X224 DRAWS",
            "at 896 the tally's digits are what the 51-glyph budget cuts"
        );
        assert!(
            dip.contains("AETHER OFF") && dip.contains("AUDIO VA0-VA2"),
            "the honesty fields outlive the tally at the dip: {dip:?}"
        );
        let six_digits = {
            let mut st = status();
            st.draws = 123_456;
            let full = status_text(&st);
            let px = Overlay::status_font_scale(896);
            let margin = (2 * Overlay::font_scale(896)).max(4);
            let avail = (896 * 4 / 3usize).saturating_sub(2 * margin);
            let text_avail = status_text_avail(avail, px).expect("the slot strip fits at 896");
            (font::text_width(&full) * px > text_avail, full.len())
        };
        assert!(
            six_digits.0,
            "the control: even a five-glyph field overflows 896 at six digits ({} chars) — the row this \
             replaced was pinning the fixture's digit count, not the player's behaviour",
            six_digits.1 - 5
        );
        // **The floor, asserted rather than hidden.** At the native 224px height there is no step left to
        // drop, so the line still truncates — and this row states exactly how far it gets, so that a future
        // change which makes it *worse* fails here instead of passing quietly.
        let smallest = rendered(224);
        assert!(
            smallest.contains("AETHER OFF") && smallest.contains("AUDIO VA0-VA2"),
            "even at the floor the two honesty fields survive, being ordered first: {smallest:?}"
        );
        assert!(
            !smallest.contains("DRAWS 1234"),
            "if the floor now fits the whole line, this test's premise has changed — re-measure it \
             rather than deleting the row: {smallest:?}"
        );
    }

    /// Drawing the whole overlay into a buffer marks pixels, never panics, and — the invariant that matters —
    /// leaves the caller's *source* frame untouched, because it only ever writes the presentation buffer.
    #[test]
    fn drawing_the_overlay_marks_the_presentation_buffer_only() {
        let (w, h) = (640usize, 480usize);
        let mut o = Overlay::new();
        o.status_line = true;
        o.push("STATE: SAVED SLOT 3", INFO);
        o.push("RESET", ACCENT);
        let mut st = status();
        st.paused = true;
        st.occupied[3] = true;
        st.occupied[7] = true;
        // Long enough paused that the banner is up too — this test is about *everything* the overlay draws.
        present_frames(&mut o, true, PAUSED_BANNER_DWELL_FRAMES);

        let source = vec![0x0012_3456u32; w * h];
        let mut present = source.clone();
        o.draw(&mut present, w, h, whole(w, h), &st);
        assert_ne!(present, source, "the overlay drew something");
        assert_eq!(
            source,
            vec![0x0012_3456u32; w * h],
            "the source frame is untouched"
        );
        // The top strip (status line) and the bottom strip (toasts) both changed.
        let top_changed = (0..h / 8).any(|y| (0..w).any(|x| present[y * w + x] != 0x0012_3456));
        let bottom_changed =
            (h - h / 8..h).any(|y| (0..w).any(|x| present[y * w + x] != 0x0012_3456));
        assert!(top_changed, "the status line drew near the top");
        assert!(bottom_changed, "the toasts drew near the bottom");
    }

    /// The paused banner is what distinguishes a paused frontend from a hung one, so it must actually appear —
    /// and must NOT appear when running.
    #[test]
    fn the_paused_banner_appears_only_while_paused() {
        let (w, h) = (640usize, 480usize);
        let mut o = Overlay::new();
        let base = vec![GROUND; w * h];

        let mut running = base.clone();
        o.draw(&mut running, w, h, whole(w, h), &status());
        assert_eq!(
            running, base,
            "nothing at all is drawn over a running, un-toasted frame"
        );

        let mut paused_st = status();
        paused_st.paused = true;
        present_frames(&mut o, true, PAUSED_BANNER_DWELL_FRAMES);
        let mut paused = base.clone();
        o.draw(&mut paused, w, h, whole(w, h), &paused_st);
        assert_ne!(paused, base, "the paused banner is on screen");
        // …and it is near the top center, where it is drawn.
        let band = h / 12;
        let centre_inked = (band..band + 40)
            .any(|y| (w / 2 - 40..w / 2 + 40).any(|x| paused[y * w + x] != GROUND));
        assert!(centre_inked, "the banner sits top-centre");
    }

    /// **The dwell.** `write_memory` is `require_paused`, so a client editing the palette pauses and
    /// resumes the machine faster than a person can read; the banner used to strobe for the whole drag.
    /// It now appears only after the machine has been *continuously* paused for
    /// `PAUSED_BANNER_DWELL_FRAMES` presented frames — and the frame before that, it is not there at all.
    #[test]
    fn the_paused_banner_waits_out_the_dwell_before_it_appears() {
        let (w, h) = (640usize, 480usize);
        let mut st = status();
        st.paused = true;
        let mut o = Overlay::new();

        present_frames(&mut o, true, PAUSED_BANNER_DWELL_FRAMES - 1);
        assert_eq!(
            ink_after(&o, w, h, &st),
            0,
            "one frame short of the dwell, the banner must not be on screen"
        );

        o.tick(true);
        let ink = ink_after(&o, w, h, &st);
        assert!(
            ink > 500,
            "on the dwell frame the banner must be up; only {ink} pixels differ from the ground"
        );
        // A long pause keeps it up — the dwell is a threshold, not a flash.
        present_frames(&mut o, true, 300);
        assert_eq!(ink_after(&o, w, h, &st), ink, "and it stays up");
    }

    /// **The strobe case, which is the bug.** A client write burst pauses for a frame and resumes, over and
    /// over. Every one of those pauses is real, and none of them may ever put the banner on screen.
    #[test]
    fn a_burst_of_one_frame_pauses_never_shows_the_banner() {
        let (w, h) = (640usize, 480usize);
        let mut st = status();
        st.paused = true;
        let mut o = Overlay::new();

        // Far more single-frame pauses than the dwell is long: if the counter did not reset, or were not
        // consulted at all, the banner would be up long before this loop ended.
        for i in 0..8 * PAUSED_BANNER_DWELL_FRAMES {
            o.tick(true);
            assert_eq!(
                ink_after(&o, w, h, &st),
                0,
                "the banner flashed during a write burst, at pause {i}"
            );
            o.tick(false);
        }
    }

    /// Resuming resets the dwell rather than pausing it: eleven paused frames, one running frame, eleven
    /// more paused frames is **not** a pause worth announcing, however the frames add up.
    #[test]
    fn resuming_resets_the_dwell_rather_than_banking_it() {
        let (w, h) = (640usize, 480usize);
        let mut st = status();
        st.paused = true;
        let mut o = Overlay::new();

        present_frames(&mut o, true, PAUSED_BANNER_DWELL_FRAMES - 1);
        o.tick(false);
        present_frames(&mut o, true, PAUSED_BANNER_DWELL_FRAMES - 1);
        assert_eq!(
            ink_after(&o, w, h, &st),
            0,
            "22 paused frames split by one running frame is not a 12-frame pause"
        );
        // …and from there the dwell completes normally, so the reset delays the banner, never suppresses it.
        o.tick(true);
        assert!(
            ink_after(&o, w, h, &st) > 500,
            "the banner must still arrive once the pause actually lasts"
        );
    }

    /// A resume takes the banner down on the very next present — the dwell governs when it *appears*, and
    /// must not turn into a trailing delay on the way out.
    #[test]
    fn the_banner_goes_down_on_the_frame_the_machine_resumes() {
        let (w, h) = (640usize, 480usize);
        let mut st = status();
        st.paused = true;
        let mut o = Overlay::new();

        present_frames(&mut o, true, PAUSED_BANNER_DWELL_FRAMES);
        assert!(ink_after(&o, w, h, &st) > 500, "the banner is up");
        o.tick(false);
        st.paused = false;
        assert_eq!(ink_after(&o, w, h, &st), 0, "and gone the moment it runs");
    }

    /// A tiny window clips the overlay rather than panicking — the window is resizable now, and a user can
    /// drag it smaller than the text.
    #[test]
    fn a_window_smaller_than_the_overlay_clips_it() {
        let mut o = Overlay::new();
        o.status_line = true;
        for i in 0..MAX_TOASTS {
            o.push(format!("A RATHER LONG NOTIFICATION NUMBER {i}"), INFO);
        }
        let mut st = status();
        st.paused = true;
        // Past the dwell, so the banner is one of the things being clipped rather than a no-op.
        present_frames(&mut o, true, PAUSED_BANNER_DWELL_FRAMES);
        for (w, h) in [(1usize, 1usize), (16, 8), (60, 40), (200, 30)] {
            let mut buf = vec![0u32; w * h];
            o.draw(&mut buf, w, h, whole(w, h), &st);
            // …and with the picture letterboxed inside it, which puts the anchors off the buffer entirely.
            o.draw(
                &mut buf,
                w,
                h,
                Rect {
                    x: w / 3,
                    y: h / 3,
                    w: w * 2,
                    h: h * 2,
                },
                &st,
            );
        }
    }

    /// **The letterbox stays black.** With the picture inset inside a larger window, every overlay element
    /// anchors to the picture: nothing is drawn in the bars above, below or beside it. That is what makes a
    /// resized (or fullscreened) window look deliberate rather than like the HUD came unmoored.
    #[test]
    fn the_overlay_anchors_to_the_picture_not_the_window() {
        let (w, h) = (800usize, 600usize);
        let mut o = Overlay::new();
        o.status_line = true;
        o.push("HELLO", INFO);
        o.push("A MUCH LONGER MESSAGE THAN ANY PICTURE HERE IS WIDE", INFO);
        let mut st = status();
        st.paused = true;
        // Past the dwell: the banner is the widest fixed-size element here, so it must be on screen for
        // this test to be testing anything about the letterbox.
        present_frames(&mut o, true, PAUSED_BANNER_DWELL_FRAMES);

        // A comfortable picture, a pillarboxed one, a letterboxed one, and two so narrow that neither the
        // status line nor the banner can fit at their natural size — the sizes at which a fixed-width element
        // would be the thing that leaks.
        for area in [
            Rect {
                x: 160,
                y: 60,
                w: 480,
                h: 480,
            },
            Rect {
                x: 40,
                y: 0,
                w: 720,
                h: 600,
            },
            Rect {
                x: 0,
                y: 150,
                w: 800,
                h: 300,
            },
            Rect {
                x: 300,
                y: 200,
                w: 140,
                h: 100,
            },
            Rect {
                x: 380,
                y: 280,
                w: 24,
                h: 20,
            },
            // Short but wide: the heights at which a fixed-height panel is the thing that leaks downward.
            Rect {
                x: 100,
                y: 100,
                w: 300,
                h: 13,
            },
            Rect {
                x: 100,
                y: 100,
                w: 300,
                h: 10,
            },
            Rect {
                x: 100,
                y: 100,
                w: 200,
                h: 1,
            },
            Rect {
                x: 100,
                y: 100,
                w: 1,
                h: 200,
            },
        ] {
            let mut buf = vec![0u32; w * h];
            o.draw(&mut buf, w, h, area, &st);
            let inked = |x: usize, y: usize| buf[y * w + x] != 0;
            for y in 0..h {
                for x in 0..w {
                    let inside =
                        x >= area.x && x < area.x + area.w && y >= area.y && y < area.y + area.h;
                    assert!(
                        inside || !inked(x, y),
                        "{area:?}: ({x},{y}) is in the letterbox but was drawn on"
                    );
                }
            }
            // …and wherever there is room for it, something *was* drawn inside the picture. (Where there is
            // not, drawing nothing is the correct outcome — the alternative is ink in the letterbox.)
            if area.w >= 140 && area.h >= 40 {
                assert!(
                    (0..h).any(|y| (0..w).any(|x| inked(x, y))),
                    "{area:?}: nothing was drawn inside the picture"
                );
            }
        }
    }

    /// A slot action flashes the status line up without latching it, so the slot strip answers "which slot,
    /// and is anything in it?" for a user who has never pressed F3 — and then gets out of the way.
    #[test]
    fn a_slot_action_flashes_the_status_line_and_then_hides_it_again() {
        let mut o = Overlay::new();
        assert!(
            !o.showing_status(),
            "off by default — a player wants the picture"
        );
        o.flash();
        assert!(o.showing_status());
        for _ in 0..TOAST_FRAMES - 1 {
            o.tick(false);
        }
        assert!(o.showing_status(), "still up one frame before it lapses");
        o.tick(false);
        assert!(!o.showing_status(), "and gone after that");

        // F3 latches it independently: a lapsed flash must not switch off a deliberately-enabled line.
        o.status_line = true;
        o.flash();
        for _ in 0..TOAST_FRAMES * 2 {
            o.tick(false);
        }
        assert!(o.showing_status(), "F3 stays on after the flash lapses");
    }

    /// `fit` keeps whole glyphs and never claims more room than it has — the guard that keeps a long message
    /// from spilling out of the picture.
    #[test]
    fn fitting_text_keeps_whole_glyphs_within_the_budget() {
        // One glyph is 5 px of ink; each further glyph adds the 6-px advance.
        assert_eq!(fit("ABCDE", 4, 1), "", "not even one glyph fits");
        assert_eq!(fit("ABCDE", 5, 1), "A");
        assert_eq!(fit("ABCDE", 10, 1), "A", "a second glyph needs 11");
        assert_eq!(fit("ABCDE", 11, 1), "AB");
        assert_eq!(
            fit("ABCDE", 1000, 1),
            "ABCDE",
            "plenty of room, nothing lost"
        );
        assert_eq!(fit("", 100, 2), "");
        // Scale multiplies the budget requirement.
        assert_eq!(fit("ABCDE", 11, 2), "A");
        assert_eq!(fit("ABCDE", 22, 2), "AB");
        // Whatever comes back really does fit.
        for avail in 0..80 {
            let got = fit("A LONGER MESSAGE", avail, 2);
            assert!(
                font::text_width(got) * 2 <= avail,
                "fit({avail}) returned {got:?}, which is wider than its budget"
            );
        }
    }

    /// `fit_marked` shows the whole string when it fits, and otherwise a visibly cut one — never a shorter
    /// string that reads as complete. Every expectation is arithmetic on `fit`'s own cost model (5 px for
    /// the first glyph, `ADVANCE` for each later one), not a measured figure.
    #[test]
    fn a_marked_fit_is_whole_or_visibly_cut_and_never_wider_than_its_budget() {
        let adv = font::ADVANCE;
        // "ABCDE" is 5 glyphs = 5 + 4*adv = 29 px at 1x. Room for all of it: untouched, and borrowed.
        assert!(matches!(
            fit_marked("ABCDE", 5 + 4 * adv, 1),
            Cow::Borrowed("ABCDE")
        ));
        // One pixel short: four glyphs would fit (5 + 3*adv = 23), but the mark is a glyph too, so three
        // glyphs plus the mark. Never four glyphs with no mark — that is the defect.
        assert_eq!(fit_marked("ABCDE", 5 + 4 * adv - 1, 1), "ABC\u{2026}");
        // Exactly one glyph plus the mark.
        assert_eq!(fit_marked("ABCDE", 5 + adv, 1), "A\u{2026}");
        // Room for one glyph but not for one glyph plus the mark: nothing, rather than a bare mark or a
        // lone letter pretending to be the message.
        assert_eq!(fit_marked("ABCDE", 5 + adv - 1, 1), "");
        assert_eq!(fit_marked("", 100, 2), "");
        // Scale multiplies the requirement the same way it does for `fit`.
        assert_eq!(fit_marked("ABCDE", (5 + adv) * 2, 2), "A\u{2026}");
        assert_eq!(fit_marked("ABCDE", (5 + 4 * adv) * 2, 2), "ABCDE");
        // Whatever comes back really does fit, and a cut one always carries the mark.
        for avail in 0..120 {
            let got = fit_marked("A LONGER MESSAGE", avail, 2);
            assert!(
                font::text_width(&got) * 2 <= avail,
                "fit_marked({avail}) returned {got:?}, which is wider than its budget"
            );
            if !got.is_empty() && got.as_ref() != "A LONGER MESSAGE" {
                assert!(
                    got.ends_with(TRUNCATION_MARK),
                    "fit_marked({avail}) cut the text without saying so: {got:?}"
                );
            }
        }
        // The mark itself is a glyph the font draws — otherwise this whole function paints a hollow box.
        assert!(
            font::has_glyph(TRUNCATION_MARK),
            "the truncation mark has no glyph in font.rs"
        );
    }

    /// **A toast that does not fit at the real toast width is cut with a visible mark, and the whole rendered
    /// string is what the arithmetic says** — asserted whole, because `contains()` is how F-TOAST-TRUNCATES
    /// hid: the old rendering `…/LOCKED (PE` contained every substring anyone checked for.
    ///
    /// The width is the one `draw` uses, via the same `font_scale`, margin and `toast_text_avail` it uses,
    /// at the player's smallest picture (224 px tall, 4:3); the glyph capacity is then derived from `fit`'s
    /// cost model, and the expected string is that many glyphs of the message minus one for the mark.
    #[test]
    fn a_toast_cut_at_the_real_toast_width_ends_with_the_mark_and_nothing_is_hidden() {
        let area = whole(224 * 4 / 3, 224);
        let px = Overlay::font_scale(area.h.max(1));
        let margin = (2 * px).max(4);
        let avail = Overlay::toast_text_avail(area, px, margin);
        // Glyphs that fit in `avail`: the first costs 5 px, each later one `ADVANCE` px.
        let capacity = if avail < 5 * px {
            0
        } else {
            1 + (avail - 5 * px) / (font::ADVANCE * px)
        };
        assert!(
            capacity >= 8,
            "COULD NOT MEASURE: {capacity} glyphs of toast room at the floor is too few to cut anything"
        );

        // A message one glyph longer than the room: numbered so a wrong cut point names itself.
        let text: String = (0..=capacity)
            .map(|i| char::from(b'A' + (i % 26) as u8))
            .collect();
        let expected: String = text
            .chars()
            .take(capacity - 1)
            .chain(std::iter::once(TRUNCATION_MARK))
            .collect();
        let mut o = Overlay::new();
        o.push(text.clone(), INFO);
        let v = o.text_surfaces(area, &status());
        let s = only(&v, crate::screen_text::Kind::Toast);
        assert_eq!(s.text, text, "the source string is the whole message");
        assert_eq!(
            s.rendered,
            expected,
            "the rendered toast must be {}-of-{} glyphs plus the mark",
            capacity - 1,
            capacity + 1
        );
        assert!(
            s.unrenderable.is_empty(),
            "the cut toast paints a hollow box: {:?}",
            s.unrenderable
        );

        // And a message that exactly fills the room is shown whole — the mark is a cost, not a habit.
        let mut o = Overlay::new();
        let exact: String = text.chars().take(capacity).collect();
        o.push(exact.clone(), INFO);
        let v = o.text_surfaces(area, &status());
        assert_eq!(
            only(&v, crate::screen_text::Kind::Toast).rendered,
            exact,
            "a toast that fits is not marked"
        );
    }

    /// The slot strip's width matches what it draws, so the status line's layout cannot overlap it.
    #[test]
    fn the_slot_strip_fits_its_declared_width() {
        for px in 1..=4 {
            let w = slot_strip_width(px) + 40;
            let h = 40;
            let mut buf = vec![0u32; w * h];
            let occupied = [
                false, true, false, false, true, false, false, false, false, true,
            ];
            draw_slot_strip(
                &mut font::Canvas::new(&mut buf, w, h),
                0,
                5,
                px,
                4,
                &occupied,
            );
            // Nothing is drawn at or past the declared width.
            let over = slot_strip_width(px);
            for y in 0..h {
                for x in over..w {
                    assert_eq!(
                        buf[y * w + x],
                        0,
                        "px={px}: ink at x={x} past the strip width"
                    );
                }
            }
            // …and something *was* drawn inside it.
            assert!(
                buf.iter().any(|&p| p != 0),
                "px={px}: the strip drew nothing"
            );
        }
    }

    /// The selected slot is visibly distinguished from an unselected occupied one, and an occupied slot from
    /// an empty one — otherwise the strip carries no information.
    #[test]
    fn the_strip_distinguishes_selected_occupied_and_empty() {
        let px = 2;
        let (w, h) = (slot_strip_width(px), 32);
        let cell = (font::ADVANCE + 2) * px;
        let render = |slot: usize, occupied: [bool; SLOT_COUNT]| {
            let mut buf = vec![0u32; w * h];
            draw_slot_strip(
                &mut font::Canvas::new(&mut buf, w, h),
                0,
                6,
                px,
                slot,
                &occupied,
            );
            buf
        };
        let mut occ = [false; SLOT_COUNT];
        occ[1] = true;
        let a = render(0, occ);
        // Cell 0 (selected, empty) has a solid box behind it; cell 2 (unselected, empty) does not.
        let box_px = |buf: &Vec<u32>, i: usize| buf[5 * w + i * cell + 1];
        assert_ne!(box_px(&a, 0), 0, "the selected slot has a filled box");
        assert_eq!(box_px(&a, 2), 0, "an unselected empty slot has no box");
        // Occupied vs empty differ in the digit colour.
        let occupied_ink: Vec<u32> = a[6 * w + cell..6 * w + 2 * cell].to_vec();
        let empty_ink: Vec<u32> = a[6 * w + 2 * cell..6 * w + 3 * cell].to_vec();
        assert_ne!(
            occupied_ink, empty_ink,
            "occupied and empty slots look different"
        );
    }

    // ---------------------------------------------------------------- the readout (§11.29, CR-H)

    /// Ink anywhere in the picture that is not the background — the whole overlay's, not one surface's.
    fn overlay_ink(o: &Overlay, w: usize, h: usize, st: &Status) -> usize {
        let mut buf = vec![GROUND; w * h];
        o.draw(&mut buf, w, h, whole(w, h), st);
        buf.iter().filter(|&&p| p != GROUND).count()
    }

    /// The one surface of a given kind, or a loud failure. "Couldn't find it" must never read as zero.
    fn only(
        v: &[crate::screen_text::Surface],
        k: crate::screen_text::Kind,
    ) -> &crate::screen_text::Surface {
        let hits: Vec<_> = v.iter().filter(|s| s.kind == k).collect();
        assert_eq!(
            hits.len(),
            1,
            "expected exactly one {k:?} surface, got {}: {v:?}",
            hits.len()
        );
        hits[0]
    }

    /// **The readout may not report text the paint did not put on the glass, and may not omit text it did.**
    ///
    /// This is the property the whole feature rests on, and it is checked against **pixels** rather than
    /// against a second copy of the layout arithmetic — which would agree with itself and prove nothing.
    /// Swept across window heights from far below the overlay's floor to well above it, so both `None`
    /// branches of `status_line_layout` (too narrow for the strip, too short for a row) are exercised
    /// alongside the ordinary case.
    ///
    /// The nothing-else-on setup is load-bearing: with no toasts, no badge and no banner, every non-ground
    /// pixel in the picture belongs to the status line, so ink is a witness for *that* surface.
    #[test]
    fn the_readout_reports_the_status_line_exactly_when_the_paint_draws_one() {
        let st = status(); // default layers, so no badge; not paused, so no banner
        let mut o = Overlay::new();
        o.status_line = true;
        let mut saw_drawn = 0;
        let mut saw_absent = 0;
        for h in [8usize, 12, 16, 24, 32, 48, 64, 96, 128, 224, 448, 672, 896] {
            let w = h * 4 / 3;
            let ink = overlay_ink(&o, w, h, &st);
            let reported = o
                .text_surfaces(whole(w, h), &st)
                .iter()
                .any(|s| s.kind == crate::screen_text::Kind::StatusLine);
            assert_eq!(
                reported,
                ink > 0,
                "{w}x{h}: the readout says {reported} and the glass has {ink} ink pixels — a readout \
                 that disagrees with the paint is exactly the lie this method exists to catch"
            );
            if reported {
                saw_drawn += 1;
            } else {
                saw_absent += 1;
            }
        }
        // Without this the row above is vacuous: a `text_surfaces` that returned nothing and a `draw` that
        // painted nothing would agree at every size, and the sweep would be green having tested neither
        // branch. Both must actually occur.
        assert!(
            saw_drawn > 0 && saw_absent > 0,
            "the sweep never saw both states ({saw_drawn} drawn, {saw_absent} absent) — widen it rather \
             than trusting it"
        );
    }

    /// **The whole rendered string, not the fields this parcel added.**
    ///
    /// At the sizes the player actually runs at, the reported `rendered` is the *entire* status line and
    /// `truncated` is false; at the 224px floor it is a strict, non-empty prefix and `truncated` is true.
    /// Both halves are asserted against [`status_text`] — the composer itself — rather than against a
    /// literal copied from a nearby pin, so a change to the line's content moves this test with it instead
    /// of leaving it agreeing with a stale transcription.
    #[test]
    fn the_readout_carries_the_source_string_and_the_prefix_that_survived() {
        let st = status();
        let full = status_text(&st);
        let mut o = Overlay::new();
        o.status_line = true;

        for h in [448usize, 672, 1080] {
            let v = o.text_surfaces(whole(h * 4 / 3, h), &st);
            let s = only(&v, crate::screen_text::Kind::StatusLine);
            assert_eq!(
                s.text, full,
                "{h}: the source string is what `status_text` composed"
            );
            assert_eq!(
                s.rendered, full,
                "{h}: a 2x window and above shows the whole line — if this now truncates, the player \
                 regressed and this is where it is visible"
            );
        }

        // **896 is the budget's dip** — `status_font_scale` steps 2→3 there while the picture grows only
        // 4:3, so the line's budget in glyphs falls to 51 before climbing again (see
        // `the_whole_status_line_survives_at_the_sizes_the_player_actually_uses`, which measures it). The
        // readout's job is the same here as anywhere: report the prefix that reached the glass, and say
        // that it is a prefix. Asserted whole, because "it truncated" is not the claim — *how far it got*
        // is, and a change that cut one field more would otherwise pass here.
        let v = o.text_surfaces(whole(896 * 4 / 3, 896), &st);
        let s = only(&v, crate::screen_text::Kind::StatusLine);
        assert_eq!(s.text, full, "896: the source is still the whole line");
        assert_eq!(
            s.rendered, "VOL 7/10 AETHER OFF AUDIO VA0-VA2 4:3 320X224 DRAWS",
            "896: the tally's digits are what the 51-glyph budget cuts"
        );
        assert!(
            full.starts_with(&s.rendered) && s.rendered.len() < full.len(),
            "896: what is reported must be a strict prefix of what was composed"
        );

        // The floor. The line does not fit here, and the readout must say so rather than reporting the
        // message as though it were on screen.
        let v = o.text_surfaces(whole(224 * 4 / 3, 224), &st);
        let s = only(&v, crate::screen_text::Kind::StatusLine);
        assert_eq!(
            s.text, full,
            "the source is the whole line even at the floor"
        );
        assert!(
            !s.rendered.is_empty()
                && s.rendered.len() < full.len()
                && full.starts_with(&s.rendered),
            "at 224px the line is cut: rendered must be a strict non-empty prefix of text, got \
             {:?} against {full:?}",
            s.rendered
        );
    }

    /// **Toasts: only the ones that reached the glass, oldest first, each with what survived.**
    ///
    /// The stack bound is the interesting half. `MAX_TOASTS` live messages in a picture with room for two
    /// rows means three of them are on no part of the screen, and a readout that listed all five would be
    /// reporting text a human cannot see — the same defect as reporting an un-truncated string.
    #[test]
    fn the_readout_lists_the_toasts_that_reached_the_glass_oldest_first() {
        let mut o = Overlay::new();
        for n in 0..MAX_TOASTS {
            o.push(format!("MESSAGE {n}"), INFO);
        }
        let area = whole(640, 480);
        let px = Overlay::font_scale(area.h.max(1));
        let margin = (2 * px).max(4);

        let v = o.text_surfaces(area, &status());
        let toasts: Vec<&crate::screen_text::Surface> = v
            .iter()
            .filter(|s| s.kind == crate::screen_text::Kind::Toast)
            .collect();
        assert_eq!(
            toasts.len(),
            o.visible_toasts(area, px, margin).len(),
            "the readout must list exactly the toasts the paint reaches, no more"
        );
        assert!(
            !toasts.is_empty(),
            "COULD NOT MEASURE: no toast reached the glass at 640x480"
        );
        // Oldest first — the order they were posted, which is the reverse of the paint's bottom-up stack.
        let texts: Vec<&str> = toasts.iter().map(|s| s.text.as_str()).collect();
        let mut sorted = texts.clone();
        sorted.sort_unstable();
        assert_eq!(texts, sorted, "posted order, oldest first: {texts:?}");

        // A picture with room for only the bottom two rows: the stack runs off the top, and the readout
        // must shorten with it rather than reporting messages that are not on screen.
        let cramped = whole(640, 32);
        let cpx = Overlay::font_scale(cramped.h.max(1));
        let n_paint = o.visible_toasts(cramped, cpx, (2 * cpx).max(4)).len();
        let n_read = o
            .text_surfaces(cramped, &status())
            .iter()
            .filter(|s| s.kind == crate::screen_text::Kind::Toast)
            .count();
        assert_eq!(
            n_read, n_paint,
            "the cramped stack must shorten the readout too"
        );
        assert!(
            n_paint < MAX_TOASTS,
            "COULD NOT MEASURE: 640x32 still fits every toast, so the bound was never exercised"
        );
    }

    /// Font scale follows the window height and stays in a sane band at both extremes.
    #[test]
    fn the_font_scale_tracks_the_window_height() {
        assert_eq!(Overlay::font_scale(224), 1);
        assert_eq!(Overlay::font_scale(672), 3);
        assert_eq!(Overlay::font_scale(50), 1, "a tiny window still gets 1x");
        assert_eq!(Overlay::font_scale(4000), 4, "a huge one is capped");
    }
}
