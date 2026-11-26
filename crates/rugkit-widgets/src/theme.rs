//! Typed design tokens: rugkit's bold, playful design language in light and dark modes,
//! with reactive system appearance tracking and subtree token overrides.
//!
//! [`theme`] is reactive: calling it inside a reactive closure (such as `.color(move || theme().ink)`)
//! automatically subscribes to system appearance changes (light/dark mode and macOS accent colour)
//! so colours spring smoothly when the OS appearance changes.
//!
//! Use [`crate::theme_scope`] to override any token (accent, dark mode, radii, springs, …) for
//! an entire subtree.

use std::cell::OnceCell;

use rugkit_reactive::{Owner, Signal, signal};
use rugkit_render::Color;
use rugkit_shell::{Appearance, ColorScheme};

use crate::Spring;

/// Corner radius tokens in logical pixels.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Radii {
    /// Small controls (checkboxes, badges).
    pub sm: f64,
    /// Medium controls (inputs, tabs, popovers).
    pub md: f64,
    /// Large surfaces (cards, modals).
    pub lg: f64,
    /// Full pill rounding (buttons, toggles).
    pub pill: f64,
}

impl Radii {
    /// Playful, generously rounded default radii.
    pub const PLAYFUL: Self = Self {
        sm: 8.0,
        md: 16.0,
        lg: 28.0,
        pill: 999.0,
    };
}

/// Spacing scale in logical pixels.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Spacing {
    /// Tight spacing (4px).
    pub xs: f64,
    /// Small spacing (8px).
    pub sm: f64,
    /// Standard spacing (14px).
    pub md: f64,
    /// Generous spacing (22px).
    pub lg: f64,
    /// Section spacing (32px).
    pub xl: f64,
}

impl Spacing {
    /// Default spacing scale.
    pub const PLAYFUL: Self = Self {
        xs: 4.0,
        sm: 8.0,
        md: 14.0,
        lg: 22.0,
        xl: 32.0,
    };
}

/// Typography size scale in logical pixels.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TypeScale {
    /// Small caption text.
    pub caption: f32,
    /// Default body copy.
    pub body: f32,
    /// Section headings.
    pub heading: f32,
    /// Large display titles.
    pub title: f32,
}

impl TypeScale {
    /// Default type scale.
    pub const PLAYFUL: Self = Self {
        caption: 13.0,
        body: 16.0,
        heading: 22.0,
        title: 34.0,
    };
}

/// The typed design tokens widgets read their defaults from.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Theme {
    /// Window background.
    pub background: Color,
    /// Raised surfaces such as cards.
    pub surface: Color,
    /// Secondary elevated surface (popovers, modals, hover fills).
    pub surface_raised: Color,
    /// Primary text and icons.
    pub ink: Color,
    /// Secondary text.
    pub ink_soft: Color,
    /// Brand accent: primary buttons, focus rings and highlights.
    pub accent: Color,
    /// Success and positive values.
    pub positive: Color,
    /// Errors and negative values.
    pub negative: Color,
    /// Warnings and cautions.
    pub warning: Color,
    /// Subtle hairline border colour.
    pub border: Color,
    /// Corner radius scale.
    pub radii: Radii,
    /// Spacing scale.
    pub spacing: Spacing,
    /// Typography scale.
    pub type_scale: TypeScale,
    /// Spring for layout, size and scale changes.
    pub layout_spring: Spring,
    /// Spring for colour changes.
    pub color_spring: Spring,
}

impl Theme {
    /// The default light theme: warm, saturated and bouncy.
    pub const PLAYFUL_LIGHT: Self = Self {
        background: Color::from_hex(0xFFF3E6),
        surface: Color::WHITE,
        surface_raised: Color::from_hex(0xFFFBF7),
        ink: Color::from_hex(0x1E1B3A),
        ink_soft: Color::from_hex(0x6B6687),
        accent: Color::from_hex(0x7B61FF),
        positive: Color::from_hex(0x00B894),
        negative: Color::from_hex(0xFF5A5F),
        warning: Color::from_hex(0xFF9F1C),
        border: Color::rgba(0.118, 0.106, 0.227, 0.12),
        radii: Radii::PLAYFUL,
        spacing: Spacing::PLAYFUL,
        type_scale: TypeScale::PLAYFUL,
        layout_spring: Spring::BOUNCY,
        color_spring: Spring::SNAPPY,
    };

    /// The default dark theme: deep indigo surfaces with crisp contrast.
    pub const PLAYFUL_DARK: Self = Self {
        background: Color::from_hex(0x141224),
        surface: Color::from_hex(0x1F1C36),
        surface_raised: Color::from_hex(0x2A2647),
        ink: Color::from_hex(0xF5F3FF),
        ink_soft: Color::from_hex(0xA6A1C4),
        accent: Color::from_hex(0x9B85FF),
        positive: Color::from_hex(0x26D9B2),
        negative: Color::from_hex(0xFF6B70),
        warning: Color::from_hex(0xFFB74D),
        border: Color::rgba(0.961, 0.953, 1.0, 0.14),
        radii: Radii::PLAYFUL,
        spacing: Spacing::PLAYFUL,
        type_scale: TypeScale::PLAYFUL,
        layout_spring: Spring::BOUNCY,
        color_spring: Spring::SNAPPY,
    };

    /// Builds a [`Theme`] matching the given OS [`Appearance`] (light/dark + accent colour).
    pub fn for_appearance(appearance: Appearance) -> Self {
        let mut base = match appearance.scheme {
            ColorScheme::Light => Self::PLAYFUL_LIGHT,
            ColorScheme::Dark => Self::PLAYFUL_DARK,
        };
        if let Some(accent) = appearance.accent {
            base.accent = accent;
        }
        base
    }

    /// Returns a copy of this theme with a different `accent` colour.
    pub const fn with_accent(mut self, accent: Color) -> Self {
        self.accent = accent;
        self
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::PLAYFUL_LIGHT
    }
}

thread_local! {
    static ACTIVE_THEME: OnceCell<Signal<Theme>> = const { OnceCell::new() };
}

fn theme_signal() -> Signal<Theme> {
    ACTIVE_THEME
        .with(|cell| *cell.get_or_init(|| Owner::root().run(|| signal(Theme::PLAYFUL_LIGHT))))
}

/// The active root theme. Reading this inside a reactive prop subscribes to system appearance
/// changes.
pub fn theme() -> Theme {
    theme_signal().get()
}

/// Updates the active theme to match `appearance` (called automatically by [`crate::Ui`]).
pub fn set_appearance(appearance: Appearance) {
    theme_signal().set_if_changed(Theme::for_appearance(appearance));
}

/// Directly overrides the active root theme (useful for in-app Light/Dark toggles or inspector).
pub fn set_theme(new_theme: Theme) {
    theme_signal().set_if_changed(new_theme);
}
