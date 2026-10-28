//! Colours, type and geometry, the same as asides' (`asides-overlay`'s
//! `controls.rs`, which started from this crate): glass the window blurs what
//! is behind it through, warm grays in dark mode and white in light mode, one
//! hairline weight, colour kept for status, and asides' soft blue for the one
//! main action.
//!
//! Where the system offers no blur through GPUI (see [`crate::glass`]), the
//! same colours are drawn opaque: [`Theme::of`] makes the surfaces solid.

use gpui::{FontWeight, Rgba, Window, WindowAppearance};

pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Rgba {
    Rgba { r, g, b, a }
}

/// `0xRRGGBB` at `alpha`.
pub const fn hex(value: u32, alpha: f32) -> Rgba {
    rgba(
        ((value >> 16) & 0xff) as f32 / 255.0,
        ((value >> 8) & 0xff) as f32 / 255.0,
        (value & 0xff) as f32 / 255.0,
        alpha,
    )
}

const fn white(alpha: f32) -> Rgba {
    rgba(1.0, 1.0, 1.0, alpha)
}

const fn black(alpha: f32) -> Rgba {
    rgba(0.0, 0.0, 0.0, alpha)
}

/// asides' own colour ("soft blue cloud"): the main action and the caret;
/// what is selected is a glass pill instead.
pub const ACCENT: u32 = 0xc2d6f6;
/// The accent hovered.
pub const ACCENT_LIGHT: u32 = 0xd1dff6;
/// The accent where the soft one is faint: a focus ring on white.
pub const ACCENT_DEEP: u32 = 0x92b6f0;
/// Text and icons on the accent.
pub const ACCENT_INK: u32 = 0x16213a;

/// Status inks as they are on dark surfaces; [`Theme::ink`] gives each the
/// deeper one light surfaces use.
pub struct Ink;

impl Ink {
    pub const FRESH: Rgba = hex(0x34c759, 1.0);
    pub const ATTENTION: Rgba = hex(0xf5a623, 1.0);
    pub const DANGER: Rgba = hex(0xf5453a, 1.0);
    /// The ball in the logo (`#4d88ff`, the icons' ball and trail).
    pub const BALL: Rgba = rgba(0.302, 0.533, 1.0, 1.0);
    pub const IDLE: Rgba = rgba(0.557, 0.557, 0.576, 1.0);
}

pub struct Radius;

impl Radius {
    pub const CHIP: f32 = 5.0;
    pub const CONTROL: f32 = 6.0;
    pub const ROW: f32 = 7.0;
    pub const CARD: f32 = 10.0;
    pub const PANEL: f32 = 12.0;
    pub const SHEET: f32 = 14.0;
}

/// Type sizes and weights (points).
pub struct Type;

impl Type {
    pub const META: f32 = 11.0;
    pub const BODY: f32 = 13.0;
    pub const TITLE: f32 = 15.0;
    pub const DISPLAY: f32 = 22.0;
    pub const MEDIUM: FontWeight = FontWeight::MEDIUM;
    pub const SEMIBOLD: FontWeight = FontWeight::SEMIBOLD;
}

/// Fixed geometry.
pub struct Metrics;

impl Metrics {
    /// The strip along the top of a column, the window's title bar: the
    /// traffic lights over the sidebar and a page's actions (a Mac, and
    /// Linux under the system's title bar); on Windows the caption buttons'
    /// height, a page's actions being beside its title there
    /// (`page_actions`). Where nothing is on it, it moves the window.
    pub const TOOLBAR: f32 = if cfg!(windows) { 32.0 } else { 44.0 };
    pub const SIDEBAR: f32 = 216.0;
    pub const NAV_ROW: f32 = 28.0;
    pub const SETTINGS_ROW: f32 = 50.0;
    pub const CONTROL: f32 = 26.0;
    /// Room for the traffic lights when the title bar is the app's own.
    pub const TRAFFIC_LIGHTS: f32 = if cfg!(target_os = "macos") { 72.0 } else { 0.0 };
    /// A caption button's width, as Windows 11 draws them (46 × 32).
    pub const CAPTION_BUTTON: f32 = 46.0;
    /// Room for the caption buttons at the top right when the title bar is
    /// the app's own (Windows).
    pub const CAPTION_BUTTONS: f32 = if cfg!(windows) {
        3.0 * Self::CAPTION_BUTTON
    } else {
        0.0
    };
    /// A page's column.
    pub const FORM: f32 = 640.0;
}

/// The order floating things paint in over the window (GPUI's `deferred`
/// priorities: higher paints on top). A menu opened from a control inside a
/// sheet must cover the sheet, so menus sit above sheets.
pub struct Layer;

impl Layer {
    pub const SHEET: usize = 2;
    /// Select menus, context menus.
    pub const MENU: usize = 3;
}

/// Every colour a view needs, for one appearance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub dark: bool,
    /// The window blurs what is behind it: the surfaces are tints over it.
    /// Without, they are solid (see [`Theme::solid_surfaces`]).
    pub glass: bool,
    /// Text and marks, from the most to the least prominent.
    pub primary: Rgba,
    pub secondary: Rgba,
    pub tertiary: Rgba,
    /// Placeholders and disabled text.
    pub quaternary: Rgba,
    /// Glass: the work surface (what is read) is the densest; the sidebar
    /// lets more of the desktop through.
    pub background: Rgba,
    pub sidebar: Rgba,
    /// Setup, read through at first launch: near solid, as over a page of
    /// text behind the window the glass let its lines show through the words.
    pub reading: Rgba,
    /// Menus, sheets and tooltips.
    pub floating: Rgba,
    pub floating_stroke: Rgba,
    /// One-point separators.
    pub hairline: Rgba,
    /// A card of rows: its fill (near solid: over a bright or busy desktop a
    /// faint one left its text hard to read) and outline.
    pub card: Rgba,
    pub card_stroke: Rgba,
    pub hover: Rgba,
    pub pressed: Rgba,
    /// The glass pill behind a selected row or segment.
    pub selected: Rgba,
    pub selected_stroke: Rgba,
    /// A quiet button. Near solid: on glass, translucent controls disappear
    /// over a light desktop.
    pub control: Rgba,
    pub control_hover: Rgba,
    pub control_stroke: Rgba,
    /// Text fields: a control's surface.
    pub field: Rgba,
    pub field_stroke: Rgba,
    /// asides' soft blue: the one main action on a page.
    pub accent: Rgba,
    pub accent_hover: Rgba,
    /// Text and marks on a fill of the accent.
    pub on_accent: Rgba,
    /// The ring around what the keyboard moved to.
    pub focus: Rgba,
    /// Status: on and well, needs attention, wrong or destructive.
    pub fresh: Rgba,
    pub attention: Rgba,
    pub danger: Rgba,
    /// Scrim behind a sheet.
    pub scrim: Rgba,
}

impl Theme {
    pub const fn dark() -> Theme {
        Theme {
            dark: true,
            glass: true,
            primary: hex(0xedebe7, 1.0),
            secondary: white(0.62),
            tertiary: white(0.42),
            quaternary: white(0.26),
            background: hex(0x1d1c1a, 0.82),
            sidebar: hex(0x262422, 0.6),
            reading: hex(0x1d1c1a, 0.96),
            floating: hex(0x33312e, 1.0),
            floating_stroke: white(0.09),
            hairline: white(0.07),
            card: hex(0x2a2826, 0.94),
            card_stroke: white(0.10),
            hover: white(0.06),
            pressed: white(0.10),
            selected: white(0.10),
            selected_stroke: white(0.09),
            control: hex(0x3a3836, 0.92),
            control_hover: hex(0x45423f, 0.95),
            control_stroke: white(0.12),
            field: hex(0x3a3836, 0.92),
            field_stroke: white(0.12),
            accent: hex(ACCENT, 1.0),
            accent_hover: hex(ACCENT_LIGHT, 1.0),
            on_accent: hex(ACCENT_INK, 1.0),
            focus: hex(ACCENT, 0.85),
            fresh: Ink::FRESH,
            attention: Ink::ATTENTION,
            danger: Ink::DANGER,
            scrim: black(0.45),
        }
    }

    pub const fn light() -> Theme {
        Theme {
            dark: false,
            glass: true,
            primary: hex(0x1c1b19, 1.0),
            secondary: black(0.58),
            tertiary: black(0.45),
            quaternary: black(0.26),
            background: hex(0xffffff, 0.86),
            sidebar: hex(0xf5f4f2, 0.74),
            reading: hex(0xffffff, 0.96),
            floating: hex(0xffffff, 1.0),
            floating_stroke: black(0.10),
            hairline: black(0.08),
            card: hex(0xffffff, 0.94),
            card_stroke: black(0.09),
            hover: black(0.045),
            pressed: black(0.08),
            selected: hex(0xffffff, 0.92),
            selected_stroke: black(0.09),
            control: hex(0xffffff, 0.94),
            control_hover: hex(0xffffff, 1.0),
            control_stroke: black(0.12),
            field: hex(0xffffff, 0.94),
            field_stroke: black(0.12),
            accent: hex(ACCENT, 1.0),
            accent_hover: hex(ACCENT_LIGHT, 1.0),
            on_accent: hex(ACCENT_INK, 1.0),
            // On white the soft blue is faint: the deeper one.
            focus: hex(ACCENT_DEEP, 1.0),
            fresh: hex(0x248a3d, 1.0),
            attention: hex(0xb26a00, 1.0),
            danger: hex(0xd70015, 1.0),
            scrim: black(0.18),
        }
    }

    /// The same colours, drawn solid: a window the system does not blur
    /// behind shows them as they are, not tints over the desktop.
    pub const fn solid_surfaces(mut self) -> Theme {
        self.glass = false;
        self.background.a = 1.0;
        self.sidebar.a = 1.0;
        self.reading.a = 1.0;
        self
    }

    /// The theme for the window's appearance (in development,
    /// `PINGPONG_APPEARANCE`'s, `light` or `dark`, whatever the system's),
    /// glass where the window is (see [`crate::glass::supported`]).
    pub fn of(window: &Window) -> Theme {
        let theme = match std::env::var("PINGPONG_APPEARANCE").as_deref() {
            Ok("light") => Theme::light(),
            Ok("dark") => Theme::dark(),
            _ => match window.appearance() {
                WindowAppearance::Dark | WindowAppearance::VibrantDark => Theme::dark(),
                WindowAppearance::Light | WindowAppearance::VibrantLight => Theme::light(),
            },
        };
        if crate::glass::supported() {
            theme
        } else {
            theme.solid_surfaces()
        }
    }

    /// A status ink that reads on this surface: as it is on dark; on light,
    /// asides' deeper ones for the three statuses, and any other pulled
    /// toward the text colour.
    pub fn ink(&self, hue: Rgba) -> Rgba {
        if self.dark {
            return hue;
        }
        let deeper = [
            (Ink::FRESH, self.fresh),
            (Ink::ATTENTION, self.attention),
            (Ink::DANGER, self.danger),
        ];
        if let Some((_, ink)) = deeper.into_iter().find(|(h, _)| *h == hue.alpha(1.0)) {
            return ink.alpha(hue.a);
        }
        let mix = |a: f32, b: f32| a * 0.62 + b * 0.38;
        rgba(
            mix(hue.r, self.primary.r),
            mix(hue.g, self.primary.g),
            mix(hue.b, self.primary.b),
            hue.a,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inks_stay_bright_on_dark_and_deepen_on_light() {
        assert_eq!(Theme::dark().ink(Ink::FRESH), Ink::FRESH);
        assert_eq!(Theme::light().ink(Ink::FRESH), hex(0x248a3d, 1.0));
        assert_eq!(Theme::light().ink(Ink::DANGER.alpha(0.5)).a, 0.5);
        assert!(Theme::light().ink(Ink::BALL).b < Ink::BALL.b);
    }

    #[test]
    fn text_tones_step_down() {
        for t in [Theme::dark(), Theme::light()] {
            assert!(t.secondary.a > t.tertiary.a && t.tertiary.a > t.quaternary.a);
        }
    }

    #[test]
    fn without_glass_the_surfaces_are_solid_in_the_same_colours() {
        let (glass, solid) = (Theme::dark(), Theme::dark().solid_surfaces());
        assert!(glass.background.a < 1.0 && solid.background.a == 1.0);
        assert_eq!(solid.sidebar.alpha(glass.sidebar.a), glass.sidebar);
        assert!(!solid.glass);
    }

    #[test]
    fn colours_are_asides_own() {
        assert_eq!(hex(0xc2d6f6, 1.0), Theme::light().accent);
        let c = hex(0x1d1c1a, 0.82);
        assert_eq!(
            (c.r * 255.0, c.g * 255.0, c.b * 255.0, c.a),
            (29.0, 28.0, 26.0, 0.82)
        );
    }
}
