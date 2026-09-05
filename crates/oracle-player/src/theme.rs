//! **The suite's design tokens, as an egui style.** `empyrean design/tokens.json` v0.2.0 read at
//! `origin/main`, mapped field by field per `docs/2026-09-05-debug-window-style.md` §1.
//!
//! Before this module the window ran egui's stock dark theme with no theming code anywhere in the crate.
//! So this is a greenfield mapping rather than an edit to a palette, and every number below is a token
//! value rather than a taste.
//!
//! # The two traps this module exists to not fall into
//!
//! 1. **`color.base.*` / `color.text.*` are not the palette.** They are an explicitly-legacy mirror of the
//!    `deep-space` family, kept alive for Aurora's and Seraph's current `gen-theme.mjs` and marked *"do not
//!    remove or deduplicate until both apps migrate"*. The forward-looking source is `surfaceFamilies`, and
//!    `surfaceFamilies.$default` is **`plum`**. A theme reading the obvious flat keys would ship the wrong
//!    family under a token name that is about to change meaning, so this module reads families and the flat
//!    keys appear nowhere in it. `DEEP_SPACE` below is the *family*, not the legacy mirror; they happen to
//!    hold the same values today and that is a fact about this revision, not a licence to read either one.
//!
//! 2. **The accent never moves with the family.** Oracle is cyan `#38BDF8` (`color.accent.oracle.value`)
//!    in every family, which is why [`ACCENT`] is a free constant and not a [`Family`] field.
//!
//! `tokens.json`'s `$meta` still describes one of its three consumers as *"a generated ImGui style for
//! Oracle"*. That branch was written for the Dear ImGui window of the legacy C++ port, now `oracle-old/`.
//! **This window is egui 0.36.1**, which is a different library with a different style model, and the third
//! branch (*"later tokens.rs for oracle-next"*) is the one this file discharges.
//!
//! # What the mapping cannot carry, stated rather than silently dropped
//!
//! * **Weight.** `type.weight` names 400/500/600; egui selects fonts by family, not by weight, and
//!   `RichText::strong()` only swaps in `Visuals::strong_text_color()` (which is *defined as*
//!   `widgets.active.text_color()`, so [`Family::text_hi`] below is what `strong()` renders). Emphasis in
//!   this window is therefore **colour and size**, never weight.
//! * **The typefaces.** `type.font` names Inter and JetBrains Mono; egui has no system-font lookup and
//!   bundles Ubuntu-Light and Hack. Shipping the suite's faces means vendoring two OFL files, which is a
//!   repo-weight and licensing call for the owner. **Until he makes it, the faces here are egui's stock
//!   pair and this module does not claim suite-font parity.** Sizes, colours and family roles are honoured.
//! * **Three motion durations.** egui has one global `Style::animation_time`. It is set to `motion.quick`;
//!   the other two live as [`INSTANT`] / [`DELIBERATE`] for per-call-site `animate_bool_with_time`.
//! * **`motion.ease`'s cubic-beziers** and **`prefers-reduced-motion`** have no egui equivalent at all.
//!   Recorded here so nobody "fixes" the omission by hand-rolling an easing layer for a debug window.
//! * **Gradients** (the titlebar wash, the accent thread, the toolbar bleed). `Frame::fill` is flat. Those
//!   belong to the frameless-chrome parcel, which is sequenced after the panels and is not this file.

use egui::{
    Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, Style, TextStyle, Vec2, Visuals,
};

/// One `surfaceFamilies` entry. Selectable per app; the accent and the semantics never change with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Family {
    /// The `surfaceFamilies` key, exactly as `tokens.json` spells it.
    pub name: &'static str,
    pub void: Color32,
    pub surface: Color32,
    pub raised: Color32,
    pub overlay: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    /// The recessed fill inputs sit on, per CHROME_SPEC's "crisp & bordered" personality.
    pub field: Color32,
    pub text_hi: Color32,
    pub text_base: Color32,
    pub text_lo: Color32,
    pub text_faint: Color32,
}

/// `#RRGGBB` as a `const fn`, so a family is a compile-time constant and a mistyped token is a mistyped
/// token rather than a runtime parse.
const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb(
        ((hex >> 16) & 0xFF) as u8,
        ((hex >> 8) & 0xFF) as u8,
        (hex & 0xFF) as u8,
    )
}

/// `surfaceFamilies.plum` — **`$default`, and therefore this window's default.**
pub const PLUM: Family = Family {
    name: "plum",
    void: rgb(0x1A0F2E),
    surface: rgb(0x2C1B46),
    raised: rgb(0x442B66),
    overlay: rgb(0x533578),
    border: rgb(0x4C3670),
    border_strong: rgb(0x614A85),
    field: rgb(0x1F1335),
    text_hi: rgb(0xF5F0FA),
    text_base: rgb(0xD8C6E6),
    text_lo: rgb(0xA88CC0),
    text_faint: rgb(0x6F5A8E),
};

/// `surfaceFamilies.deep-space`. **The family**, read from `surfaceFamilies`; the identically-valued
/// `color.base`/`color.text` block is the legacy mirror and is deliberately not read anywhere here.
/// ⚑ **Held, not yet wired.** [`install`] takes a family and this window always passes
/// [`DEFAULT_FAMILY`]: nothing here persists a preference (style page §2 item 11 parks that with the
/// `eframe` layout store). The other three are kept because this module is a mapping of
/// `surfaceFamilies`, and a mapping that could express only its own default would not be one. They are
/// exercised by [`tests::the_accent_is_cyan_in_every_family`], which is the assertion that a family
/// swap moves the surfaces and never the accent.
#[allow(dead_code)]
pub const DEEP_SPACE: Family = Family {
    name: "deep-space",
    void: rgb(0x0A0C12),
    surface: rgb(0x12151E),
    raised: rgb(0x1A1E2A),
    overlay: rgb(0x222736),
    border: rgb(0x2A2F3D),
    border_strong: rgb(0x3A4152),
    field: rgb(0x0E1118),
    text_hi: rgb(0xE8EAF2),
    text_base: rgb(0xB8BECE),
    text_lo: rgb(0x6E7589),
    text_faint: rgb(0x474D5E),
};

/// `surfaceFamilies.indigo`.
/// ⚑ **Held, not yet wired.** [`install`] takes a family and this window always passes
/// [`DEFAULT_FAMILY`]: nothing here persists a preference (style page §2 item 11 parks that with the
/// `eframe` layout store). The other three are kept because this module is a mapping of
/// `surfaceFamilies`, and a mapping that could express only its own default would not be one. They are
/// exercised by [`tests::the_accent_is_cyan_in_every_family`], which is the assertion that a family
/// swap moves the surfaces and never the accent.
#[allow(dead_code)]
pub const INDIGO: Family = Family {
    name: "indigo",
    void: rgb(0x101030),
    surface: rgb(0x1D1D46),
    raised: rgb(0x30306A),
    overlay: rgb(0x3B3B80),
    border: rgb(0x3A3A6E),
    border_strong: rgb(0x4A4A8C),
    field: rgb(0x14143A),
    text_hi: rgb(0xF0F0FA),
    text_base: rgb(0xC6C6E2),
    text_lo: rgb(0x8A8AB8),
    text_faint: rgb(0x5C5C8A),
};

/// `surfaceFamilies.twilight`.
/// ⚑ **Held, not yet wired.** [`install`] takes a family and this window always passes
/// [`DEFAULT_FAMILY`]: nothing here persists a preference (style page §2 item 11 parks that with the
/// `eframe` layout store). The other three are kept because this module is a mapping of
/// `surfaceFamilies`, and a mapping that could express only its own default would not be one. They are
/// exercised by [`tests::the_accent_is_cyan_in_every_family`], which is the assertion that a family
/// swap moves the surfaces and never the accent.
#[allow(dead_code)]
pub const TWILIGHT: Family = Family {
    name: "twilight",
    void: rgb(0x0C1230),
    surface: rgb(0x172246),
    raised: rgb(0x263768),
    overlay: rgb(0x2F447E),
    border: rgb(0x2E3F6E),
    border_strong: rgb(0x3B518C),
    field: rgb(0x101A3A),
    text_hi: rgb(0xEFF2FA),
    text_base: rgb(0xC2CCE4),
    text_lo: rgb(0x8494BC),
    text_faint: rgb(0x56638C),
};

/// Every family `tokens.json` publishes, so a chooser cannot offer a family the tokens lack.
/// ⚑ **Held, not yet wired.** [`install`] takes a family and this window always passes
/// [`DEFAULT_FAMILY`]: nothing here persists a preference (style page §2 item 11 parks that with the
/// `eframe` layout store). The other three are kept because this module is a mapping of
/// `surfaceFamilies`, and a mapping that could express only its own default would not be one. They are
/// exercised by [`tests::the_accent_is_cyan_in_every_family`], which is the assertion that a family
/// swap moves the surfaces and never the accent.
#[allow(dead_code)]
pub const FAMILIES: [Family; 4] = [PLUM, DEEP_SPACE, INDIGO, TWILIGHT];

/// `surfaceFamilies.$default`.
pub const DEFAULT_FAMILY: Family = PLUM;

/// `color.accent.oracle.value` — cyan. **Fixed:** it does not change with the family, and every other
/// tool's accent is a different constant in a different repo.
pub const ACCENT: Color32 = rgb(0x38BDF8);

/// `color.semantic.success`. Neither this nor [`INFO`] has a `Visuals` slot, so they are per-call-site
/// colours that a panel reaches for by name rather than fields the theme installs.
pub const SUCCESS: Color32 = rgb(0x34D399);
/// `color.semantic.warning` — also installed as `Visuals::warn_fg_color`.
pub const WARNING: Color32 = rgb(0xFBBF24);
/// `color.semantic.error` — also installed as `Visuals::error_fg_color`.
pub const ERROR: Color32 = rgb(0xF87171);
/// `color.semantic.info`. Equal to [`ACCENT`]'s value in this revision of `tokens.json` and **not by
/// definition** — the accent is per-tool and this is not, so a panel meaning "informational" must say
/// `INFO` and a panel meaning "Oracle" must say `ACCENT`, however identical they look today.
///
/// Held rather than wired: no panel restyled so far has an informational colour distinct from its accent.
#[allow(dead_code)]
pub const INFO: Color32 = rgb(0x38BDF8);

/// `motion.duration.instant`, in seconds. Hover and focus.
/// ⚑ **A number that exists because egui cannot theme it.** `Style::animation_time` is a single
/// global, so only `motion.quick` reaches the style; the other two durations are per-call-site values
/// for `Context::animate_bool_with_time`. Recorded here so `motion.duration` survives the mapping
/// intact rather than being silently reduced to one value.
#[allow(dead_code)]
pub const INSTANT: f32 = 0.080;
/// `motion.duration.quick`, in seconds. Panel collapse, palette, tab switch. This is the one egui's single
/// global `animation_time` gets.
pub const QUICK: f32 = 0.150;
/// `motion.duration.deliberate`, in seconds. Dialogs and theme changes.
/// ⚑ **A number that exists because egui cannot theme it.** `Style::animation_time` is a single
/// global, so only `motion.quick` reaches the style; the other two durations are per-call-site values
/// for `Context::animate_bool_with_time`. Recorded here so `motion.duration` survives the mapping
/// intact rather than being silently reduced to one value.
#[allow(dead_code)]
pub const DELIBERATE: f32 = 0.250;

/// `chrome.selectionAlpha` (0.28) as an 8-bit alpha: `0.28 * 255` rounds to 71.
const SELECTION_ALPHA: u8 = 71;

/// The accent at `chrome.selectionAlpha` — the selection wash, and the fill a chosen table row takes.
///
/// A function rather than a `const` because `Color32::from_rgba_unmultiplied` is not `const` in epaint
/// 0.36, and hand-premultiplying the three channels here would put three rounded numbers in the source
/// that nothing checks against the token.
pub fn selection() -> Color32 {
    Color32::from_rgba_unmultiplied(0x38, 0xBD, 0xF8, SELECTION_ALPHA)
}

/// `type.scale.xs` at proportional — the dense cell face for table bodies.
pub const DENSE: &str = "dense";
/// `type.scale.xl` at proportional — a section header inside a panel body.
pub const SECTION: &str = "section";

/// `radius.md` (4px) — CHROME_SPEC's "radius 4px on controls".
const RADIUS_MD: CornerRadius = CornerRadius::same(4);

/// The whole style for one family.
///
/// Built from [`Visuals::dark`] rather than from `Visuals::default()`, and `dark_mode` is left `true`
/// deliberately: several widgets branch on that flag independently of which style slot they were read
/// from, so a dark palette under `dark_mode: false` renders a handful of them wrong.
pub fn style(f: Family) -> Style {
    let mut v = Visuals::dark();

    // 1.1 surfaces.
    v.dark_mode = true;
    v.window_fill = f.void;
    v.panel_fill = f.surface;
    v.extreme_bg_color = f.field;
    v.text_edit_bg_color = Some(f.field);
    v.faint_bg_color = f.raised;
    v.code_bg_color = f.raised;
    v.window_stroke = Stroke::new(1.0, f.border);
    v.window_corner_radius = RADIUS_MD;
    v.menu_corner_radius = RADIUS_MD;
    // Depth is the surface ladder void -> surface -> raised -> overlay, which is what the families are for.
    // CHROME_SPEC forbids drop shadows on controls and there is no shadow token to convert anyway.
    v.window_shadow = egui::epaint::Shadow::NONE;
    v.popup_shadow = egui::epaint::Shadow::NONE;
    // Grid and table striping reads `faint_bg_color`, which is `raised` one step above `surface`.
    v.striped = true;

    // 1.2 widgets. `bg_fill` and `weak_bg_fill` are set together per row because `Button` reads the weak
    // one while checkbox and radio bodies read the strong one; a difference between them here would be a
    // difference nobody chose.
    let w = &mut v.widgets;
    for s in [
        &mut w.noninteractive,
        &mut w.inactive,
        &mut w.hovered,
        &mut w.active,
        &mut w.open,
    ] {
        s.corner_radius = RADIUS_MD;
        // CHROME_SPEC: controls do not grow on hover. The state is carried by stroke and fill.
        s.expansion = 0.0;
    }
    w.noninteractive.bg_fill = f.surface;
    w.noninteractive.weak_bg_fill = f.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, f.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, f.text_base);

    w.inactive.bg_fill = f.raised;
    w.inactive.weak_bg_fill = f.raised;
    w.inactive.bg_stroke = Stroke::new(1.0, f.border);
    w.inactive.fg_stroke = Stroke::new(1.0, f.text_base);

    w.hovered.bg_fill = f.overlay;
    w.hovered.weak_bg_fill = f.overlay;
    w.hovered.bg_stroke = Stroke::new(1.0, f.border_strong);
    w.hovered.fg_stroke = Stroke::new(1.0, f.text_hi);

    w.active.bg_fill = f.overlay;
    w.active.weak_bg_fill = f.overlay;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT);
    // `Visuals::strong_text_color()` IS `widgets.active.text_color()`, so this line is what every
    // `ui.strong(..)` in every panel renders as. It is the emphasis channel, standing in for the weight
    // axis egui does not have.
    w.active.fg_stroke = Stroke::new(1.0, f.text_hi);

    w.open.bg_fill = f.overlay;
    w.open.weak_bg_fill = f.overlay;
    w.open.bg_stroke = Stroke::new(1.0, f.border_strong);
    w.open.fg_stroke = Stroke::new(1.0, f.text_hi);

    // 1.3 accent, selection, semantics.
    v.selection.bg_fill = selection();
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    v.warn_fg_color = WARNING;
    v.error_fg_color = ERROR;
    v.text_cursor.stroke.color = ACCENT;
    // `text.lo` rather than an alpha of `text.base`: the token file publishes a fourth text step and a
    // computed dim would not be it.
    v.weak_text_color = Some(f.text_lo);

    let mut style = Style {
        visuals: v,
        ..Style::default()
    };

    // 1.4 text. `type.scale` converts one to one: egui points are CSS px at pixels_per_point 1.0, and the
    // scale is px throughout.
    let t = &mut style.text_styles;
    t.insert(TextStyle::Small, FontId::proportional(10.0));
    t.insert(TextStyle::Body, FontId::proportional(13.0));
    t.insert(TextStyle::Button, FontId::proportional(13.0));
    t.insert(TextStyle::Heading, FontId::proportional(16.0));
    t.insert(
        TextStyle::Monospace,
        FontId::new(11.0, FontFamily::Monospace),
    );
    t.insert(TextStyle::Name(DENSE.into()), FontId::proportional(11.0));
    t.insert(TextStyle::Name(SECTION.into()), FontId::proportional(20.0));

    // 1.5 spacing. `space` is a px scale (1 = 2px .. 9 = 48px).
    let sp = &mut style.spacing;
    sp.item_spacing = Vec2::new(4.0, 4.0);
    sp.window_margin = Margin::same(12);
    sp.menu_margin = Margin::same(12);
    sp.button_padding = Vec2::new(12.0, 4.0);
    sp.indent = 12.0;
    // A 24px `chrome.statusbar` needs rows that fit inside it, so the floor is 22 rather than egui's 24.
    sp.interact_size = Vec2::new(0.0, 22.0);
    // CHROME_SPEC: stock scrollbars are forbidden. A 6px non-floating bar whose thumb is always at full
    // opacity, because "one surface step lighter than its container" is a statement about a thumb that is
    // there, and a fading thumb is not there most of the time.
    sp.scroll.floating = false;
    sp.scroll.bar_width = 6.0;
    sp.scroll.foreground_color = true;
    sp.scroll.dormant_handle_opacity = 1.0;
    sp.scroll.active_handle_opacity = 1.0;
    sp.scroll.interact_handle_opacity = 1.0;

    style.animation_time = QUICK;
    style
}

/// Install `family` on `ctx`.
///
/// ⚑ **`set_style_of(Theme::Dark, ..)`, never bare `set_visuals`.** `Context::set_visuals` writes only the
/// slot for `self.theme()`, so a window themed that way silently reverts to stock egui the moment the
/// desktop reports a light preference. Pinning the preference to Dark *and* writing the Dark slot makes the
/// two agree no matter what the desktop says, and
/// [`a_light_desktop_cannot_revert_the_window_to_stock_egui`](tests::a_light_desktop_cannot_revert_the_window_to_stock_egui)
/// poses that desktop rather than asserting the call shape.
///
/// *(The style page's §1.7 warns against "bare `ctx.set_visuals` / `ctx.set_style`". `Context::set_style`
/// does not exist in egui 0.36.1 at all -- the pair is `set_style_of` / `all_styles_mut`, and only
/// `set_visuals` is the live hazard.)*
pub fn install(ctx: &egui::Context, family: Family) {
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_style_of(egui::Theme::Dark, style(family));
}

/// The dock chrome for `family`, derived from the egui style rather than written twice.
///
/// `egui_dock::Style::from_egui` takes the whole look from the theme already; the three overrides after it
/// are the ones CHROME_SPEC names and `from_egui` cannot infer: the tab strip sits on `void` (a step below
/// the panel body it labels), its hairline is `border`, and the active tab is marked by a 2px accent line.
pub fn dock_style(base: &Style, family: Family) -> egui_dock::Style {
    let mut s = egui_dock::Style::from_egui(base);
    s.tab_bar.bg_fill = family.void;
    s.tab_bar.hline_color = family.border;
    s.main_surface_border_stroke = Stroke::new(1.0, family.border);
    s.main_surface_border_rounding = RADIUS_MD;
    // CHROME_SPEC's "active tab gets a 2px accent underline". egui_dock draws the active tab's outline
    // from `tab.active`, so the accent goes on that tab's own stroke rather than on a separate rule this
    // crate would have to paint and keep aligned.
    s.tab.active.text_color = family.text_hi;
    s.tab.active.bg_fill = family.surface;
    s.tab.active.outline_color = ACCENT;
    s.tab.inactive.text_color = family.text_lo;
    s.tab.inactive.bg_fill = family.void;
    s.tab.focused.text_color = family.text_hi;
    s.tab.focused.outline_color = ACCENT;
    s.tab.hovered.text_color = family.text_hi;
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default family is `plum`, and it is **not** the legacy `color.base` mirror.
    ///
    /// This is the whole of trap 1 as an assertion. `tokens.json` publishes `color.base.surface` as
    /// `#12151E` and `surfaceFamilies.plum.surface` as `#2C1B46`; a theme that read the obvious flat keys
    /// would ship the first, look entirely plausible, and be the wrong family.
    #[test]
    fn the_default_family_is_plum_and_not_the_legacy_flat_tokens() {
        assert_eq!(DEFAULT_FAMILY.name, "plum");
        assert_eq!(DEFAULT_FAMILY.surface, rgb(0x2C1B46));
        assert_ne!(
            DEFAULT_FAMILY.surface, DEEP_SPACE.surface,
            "the default family must not be the deep-space values color.base mirrors"
        );
        let s = style(DEFAULT_FAMILY);
        assert_eq!(s.visuals.panel_fill, rgb(0x2C1B46));
        assert_eq!(s.visuals.window_fill, rgb(0x1A0F2E));
    }

    /// The accent is Oracle's in every family. A family swap must never move it.
    #[test]
    fn the_accent_is_cyan_in_every_family() {
        for f in FAMILIES {
            let s = style(f);
            assert_eq!(s.visuals.selection.stroke.color, ACCENT, "{}", f.name);
            assert_eq!(s.visuals.hyperlink_color, ACCENT, "{}", f.name);
            assert_eq!(
                s.visuals.widgets.active.bg_stroke.color, ACCENT,
                "{}",
                f.name
            );
        }
    }

    /// Every family really is a different palette, so the parameter is not decorative.
    #[test]
    fn the_four_families_are_four_palettes() {
        for (i, a) in FAMILIES.iter().enumerate() {
            for b in &FAMILIES[i + 1..] {
                assert_ne!(a.name, b.name, "{} listed twice", a.name);
                assert_ne!(
                    a.surface, b.surface,
                    "{} and {} share a surface, so choosing between them would do nothing",
                    a.name, b.name
                );
            }
        }
    }

    /// `type.scale`, checked at the sizes the mapping names. `Small` is 10 rather than egui's stock 9,
    /// which is the one place the mapping deliberately raises a stock value (`2xs` is 10px).
    #[test]
    fn the_text_scale_is_the_tokens_scale() {
        let s = style(DEFAULT_FAMILY);
        let size = |t: TextStyle| s.text_styles.get(&t).expect("text style").size;
        assert_eq!(size(TextStyle::Small), 10.0);
        assert_eq!(size(TextStyle::Body), 13.0);
        assert_eq!(size(TextStyle::Button), 13.0);
        assert_eq!(size(TextStyle::Heading), 16.0);
        assert_eq!(size(TextStyle::Monospace), 11.0);
        assert_eq!(size(TextStyle::Name(DENSE.into())), 11.0);
        assert_eq!(size(TextStyle::Name(SECTION.into())), 20.0);
        assert_eq!(
            s.text_styles
                .get(&TextStyle::Monospace)
                .expect("mono")
                .family,
            FontFamily::Monospace,
            "the mono style must actually be monospace; a table of addresses stops lining up otherwise"
        );
    }

    /// `chrome.selectionAlpha` is 0.28, and 0.28 of 255 is 71. A selection wash at full alpha would hide
    /// the row it is selecting.
    ///
    /// The colour is compared with a tolerance of one 8-bit step, and that is a property of `Color32`
    /// rather than a slack assertion: it stores **premultiplied** channels, so a round trip through
    /// `to_srgba_unmultiplied` at alpha 71 cannot return the exact input. One step is the widest that
    /// round trip can move a channel; "some other cyan" is many steps away and still fails.
    #[test]
    fn the_selection_wash_carries_the_tokens_alpha() {
        let s = selection();
        assert_eq!(
            s.a(),
            71,
            "the wash must carry chrome.selectionAlpha, not full alpha"
        );
        let got = s.to_srgba_unmultiplied();
        for (i, want) in [0x38u8, 0xBD, 0xF8].into_iter().enumerate() {
            assert!(
                got[i].abs_diff(want) <= 1,
                "channel {i} of the wash is {} and the accent's is {want}: this is not the accent",
                got[i]
            );
        }
        assert_eq!(style(DEFAULT_FAMILY).visuals.selection.bg_fill, s);
    }

    /// ⚑ **A light desktop cannot revert this window to stock egui.**
    ///
    /// `Context::set_visuals` and `Context::set_style` write only `self.theme()`'s slot, and under the
    /// default `ThemePreference::System` that slot is whichever one the desktop asks for. A window themed
    /// with those and nothing else renders **stock egui** on a light desktop while every assertion about
    /// `style(..)` above stays green: the theme is perfect and nobody is looking at it.
    ///
    /// So this **poses the light desktop** rather than asserting a call shape. `Options::system_theme` is
    /// `pub(crate)`, but `fallback_theme` is what `ThemePreference::System` resolves through when no system
    /// theme has arrived, so setting it to `Light` reaches the same resolution branch a light desktop does.
    /// The assertion is on the **effective** style, which is the thing a person sees.
    #[test]
    fn a_light_desktop_cannot_revert_the_window_to_stock_egui() {
        let ctx = egui::Context::default();
        ctx.options_mut(|o| o.fallback_theme = egui::Theme::Light);
        install(&ctx, DEFAULT_FAMILY);

        assert_eq!(
            ctx.theme(),
            egui::Theme::Dark,
            "the preference was not pinned, so the desktop still chooses which slot renders"
        );
        assert_eq!(
            ctx.global_style().visuals.panel_fill,
            DEFAULT_FAMILY.surface,
            "the EFFECTIVE style on a light desktop is not the theme, so the window is stock egui"
        );
        let dark = ctx.style_of(egui::Theme::Dark);
        assert_eq!(dark.visuals.panel_fill, DEFAULT_FAMILY.surface);
        assert_eq!(dark.visuals.selection.stroke.color, ACCENT);
    }

    /// The dock takes the theme, and the active tab is the accent-marked one.
    #[test]
    fn the_dock_follows_the_theme_and_marks_the_active_tab_with_the_accent() {
        let base = style(DEFAULT_FAMILY);
        let d = dock_style(&base, DEFAULT_FAMILY);
        assert_eq!(d.tab_bar.bg_fill, DEFAULT_FAMILY.void);
        assert_eq!(d.tab_bar.hline_color, DEFAULT_FAMILY.border);
        assert_eq!(d.tab.active.outline_color, ACCENT);
        assert_ne!(
            d.tab.active.bg_fill, d.tab.inactive.bg_fill,
            "an active tab that fills like an inactive one is not marked at all"
        );
    }

    /// The three motion durations exist as numbers even though egui can theme only one of them.
    #[test]
    fn the_one_animation_egui_has_is_the_quick_one() {
        assert_eq!(style(DEFAULT_FAMILY).animation_time, QUICK);
        assert_eq!((INSTANT, QUICK, DELIBERATE), (0.080, 0.150, 0.250));
    }
}
