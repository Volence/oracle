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
//! The `PAUSED` banner is the only thing that distinguishes them.
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
use std::collections::VecDeque;

/// How long a toast stays up, in presented frames (~2.5 s at 60 fps). Long enough to read a save
/// confirmation, short enough that a burst of slot presses does not bury the picture.
pub const TOAST_FRAMES: u32 = 150;

/// The last of a toast's life spent fading out, in frames (~0.5 s), so messages leave gently instead of
/// blinking off.
const FADE_FRAMES: u32 = 30;

/// How many toasts are shown at once. Older ones are dropped from the top when a burst overflows this.
pub const MAX_TOASTS: usize = 5;

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
    pub frame: u64,
    /// The selected save-state slot.
    pub slot: usize,
    /// Which slots have a file on disk. Re-probed only when it can have changed (a save, a load, a slot
    /// change), never per frame.
    pub occupied: [bool; SLOT_COUNT],
    /// Volume step and mute, as `Some((step, max, muted))`; `None` in a build with no audio feature.
    pub volume: Option<(u8, u8, bool)>,
    /// The console output-filter revision in use ("VA0-VA2", "off", …), for the status line.
    pub filter: Option<&'static str>,
    /// The display aspect mode's short name.
    pub aspect: &'static str,
    /// The native frame size currently being presented.
    pub native: (usize, usize),
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

    /// Age every toast by one presented frame and retire the expired ones. Driven from the present, not the
    /// emulated frame, so notifications last the same wall-clock time whether the audio pacer asked for 0, 1
    /// or 2 emulated frames this iteration — and so they still expire while paused.
    pub fn tick(&mut self) {
        for t in &mut self.toasts {
            t.ttl = t.ttl.saturating_sub(1);
        }
        // `retain` rather than popping from the front: a duplicate `push` refreshes a toast in place, so the
        // queue is not necessarily ordered by remaining life and the expired one can be anywhere in it.
        self.toasts.retain(|t| t.ttl > 0);
        self.status_flash = self.status_flash.saturating_sub(1);
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

    /// Draw the whole overlay into the `w * h` presentation buffer, anchored to `area` — the rectangle the
    /// game's picture occupies. Anything landing outside the buffer is clipped by [`font`], so a picture
    /// larger than the window (or a window smaller than the text) is safe.
    pub fn draw(&self, buf: &mut [u32], w: usize, h: usize, area: Rect, st: &Status) {
        let px = Self::font_scale(area.h.max(1));
        let margin = (2 * px).max(4);
        let mut c = font::Canvas::new(buf, w, h);

        if self.showing_status() {
            self.draw_status_line(&mut c, area, st, px, margin);
        }
        if st.paused {
            draw_paused_banner(&mut c, area, px);
        }
        self.draw_toasts(&mut c, area, px, margin);
    }

    /// The persistent status line (F3): slot strip, volume, filter, aspect, native size, frame counter.
    fn draw_status_line(
        &self,
        c: &mut font::Canvas,
        area: Rect,
        st: &Status,
        px: usize,
        margin: usize,
    ) {
        let pad = 2 * px;
        let strip_w = slot_strip_width(px);
        // Everything is fitted to the picture's width, so the status line can never run into the letterbox.
        let avail = area.w.saturating_sub(2 * margin);
        if avail < strip_w + 4 * pad {
            // Not even the slot strip fits. The strip is drawn as fixed-width boxes rather than text, so
            // there is nothing to truncate — drop the whole line instead of letting it run off the picture.
            return;
        }
        let full = status_text(st);
        let line = fit(&full, avail.saturating_sub(strip_w + pad + 2 * pad), px);
        let panel_w = (strip_w + pad + font::text_width(line) * px + 2 * pad).min(avail.max(1));
        let panel_h = font::GLYPH_H * px + 2 * pad;
        if area.h < margin + panel_h {
            return; // the picture is too short for a line of text to sit inside it
        }
        let ox = (area.x + margin) as i32;
        let oy = (area.y + margin) as i32;
        c.fill_rect(ox, oy, panel_w, panel_h, 0x0000_0000, font::PANEL_ALPHA);
        let x = ox + pad as i32;
        let y = oy + pad as i32;
        draw_slot_strip(c, x, y, px, st.slot, &st.occupied);
        c.text(x + (strip_w + pad) as i32, y, px, INFO, line);
    }

    /// Toasts, stacked upward from the bottom-left corner with the newest at the bottom.
    fn draw_toasts(&self, c: &mut font::Canvas, area: Rect, px: usize, margin: usize) {
        let pad = 2 * px;
        let row_h = font::LINE_H * px + 2 * pad;
        let left = (area.x + margin) as i32;
        let bottom = (area.y + area.h) as i32 - margin as i32;
        for (i, t) in self.toasts().rev().enumerate() {
            let y = bottom - ((i + 1) * row_h) as i32;
            if y < area.y as i32 {
                break; // the stack has reached the top of the picture — never spill into the letterbox
            }
            let alpha = t.alpha();
            let text = fit(&t.text, area.w.saturating_sub(2 * margin + 2 * pad), px);
            if text.is_empty() {
                continue; // the picture is too narrow for even one glyph — draw no bare panel either
            }
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
pub fn status_text(st: &Status) -> String {
    let mut s = String::new();
    if let Some((v, max, muted)) = st.volume {
        if muted {
            s.push_str("MUTE ");
        } else {
            s.push_str(&format!("VOL {v}/{max} "));
        }
    }
    if let Some(f) = st.filter {
        s.push_str(f);
        s.push(' ');
    }
    s.push_str(&format!(
        "{} {}X{} F{}",
        st.aspect, st.native.0, st.native.1, st.frame
    ));
    s
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

/// The centered `PAUSED` banner. Since a paused frontend re-presents the retained framebuffer forever, this
/// is the only thing that tells a paused emulator apart from a hung one.
fn draw_paused_banner(c: &mut font::Canvas, area: Rect, px: usize) {
    const WORD: &str = "PAUSED";
    // A twelfth of the way down the picture, so it clears the game's own top-of-screen HUD.
    let y_off = area.h / 12;
    let head_room = area.h - y_off;
    // Twice the overlay scale — the one thing on screen that should be unmissable — stepped down until the
    // whole word *and* its panel fit inside the picture on both axes. The word is never truncated: "PAU" is
    // not a pause indicator. A picture with no room for it at 1x gets nothing rather than a leak into the
    // letterbox, which at that size is a handful of pixels nobody could read anyway.
    let pad_for = |p: usize| 3 * p;
    let Some(px) = (1..=px * 2).rev().find(|&p| {
        font::text_width(WORD) * p + 2 * pad_for(p) <= area.w
            && font::GLYPH_H * p + 2 * pad_for(p) <= head_room
    }) else {
        return;
    };
    let pad = pad_for(px);
    let panel_w = font::text_width(WORD) * px + 2 * pad;
    let panel_h = font::GLYPH_H * px + 2 * pad;
    let x = (area.x + area.w.saturating_sub(panel_w) / 2) as i32;
    let y = (area.y + y_off) as i32;
    c.fill_rect(x, y, panel_w, panel_h, 0x0000_0000, 210);
    c.text(x + pad as i32, y + pad as i32, px, ACCENT, WORD);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The picture filling the whole `w x h` buffer — the common case, and the one most assertions want.
    fn whole(w: usize, h: usize) -> Rect {
        Rect { x: 0, y: 0, w, h }
    }

    fn status() -> Status {
        Status {
            paused: false,
            frame: 1234,
            slot: 3,
            occupied: [false; SLOT_COUNT],
            volume: Some((7, 10, false)),
            filter: Some("VA0-VA2"),
            aspect: "4:3",
            native: (320, 224),
        }
    }

    /// A toast lives for `TOAST_FRAMES` presented frames and then retires itself.
    #[test]
    fn a_toast_expires_after_its_lifetime() {
        let mut o = Overlay::new();
        o.push("SAVED SLOT 3", INFO);
        assert_eq!(o.toasts().count(), 1);
        for _ in 0..TOAST_FRAMES - 1 {
            o.tick();
        }
        assert_eq!(o.toasts().count(), 1, "still up one frame before expiry");
        o.tick();
        assert_eq!(o.toasts().count(), 0, "and gone on the frame it expires");
    }

    /// Repeating the same message refreshes it rather than stacking duplicates — holding `=` to ramp the
    /// volume must not paper the screen with ten identical lines.
    #[test]
    fn an_identical_message_refreshes_instead_of_stacking() {
        let mut o = Overlay::new();
        o.push("VOLUME 5/10", INFO);
        for _ in 0..100 {
            o.tick();
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
            o.tick();
        }
        let a = o.toasts().next().unwrap().alpha();
        assert!(a < 255, "it has started fading, got {a}");
        for _ in 0..FADE_FRAMES - 3 {
            o.tick();
        }
        assert!(o.toasts().next().unwrap().alpha() < 40, "nearly gone");
    }

    /// The status text names every piece of state the keyboard controls steer, and the mute case replaces the
    /// level rather than showing a stale number next to "MUTE".
    #[test]
    fn the_status_text_reports_the_steerable_state() {
        let s = status_text(&status());
        for want in ["VOL 7/10", "VA0-VA2", "4:3", "320X224", "F1234"] {
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
        let o = Overlay::new();
        let base = vec![0u32; w * h];

        let mut running = base.clone();
        o.draw(&mut running, w, h, whole(w, h), &status());
        assert_eq!(
            running, base,
            "nothing at all is drawn over a running, un-toasted frame"
        );

        let mut paused_st = status();
        paused_st.paused = true;
        let mut paused = base.clone();
        o.draw(&mut paused, w, h, whole(w, h), &paused_st);
        assert_ne!(paused, base, "the paused banner is on screen");
        // …and it is near the top center, where it is drawn.
        let band = h / 12;
        let centre_inked =
            (band..band + 40).any(|y| (w / 2 - 40..w / 2 + 40).any(|x| paused[y * w + x] != 0));
        assert!(centre_inked, "the banner sits top-centre");
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
            o.tick();
        }
        assert!(o.showing_status(), "still up one frame before it lapses");
        o.tick();
        assert!(!o.showing_status(), "and gone after that");

        // F3 latches it independently: a lapsed flash must not switch off a deliberately-enabled line.
        o.status_line = true;
        o.flash();
        for _ in 0..TOAST_FRAMES * 2 {
            o.tick();
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

    /// Font scale follows the window height and stays in a sane band at both extremes.
    #[test]
    fn the_font_scale_tracks_the_window_height() {
        assert_eq!(Overlay::font_scale(224), 1);
        assert_eq!(Overlay::font_scale(672), 3);
        assert_eq!(Overlay::font_scale(50), 1, "a tiny window still gets 1x");
        assert_eq!(Overlay::font_scale(4000), 4, "a huge one is capped");
    }
}
