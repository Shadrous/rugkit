//! Design tokens: rugkit's bold, playful design language in light and dark modes.
//!
//! [`theme`] is reactive: calling it inside a reactive closure (such as `.color(move || theme().ink)`)
//! automatically subscribes to system appearance changes (light/dark mode and macOS accent colour)
//! so colours spring smoothly when the OS appearance changes. Subtree token overrides arrive in M3.

use std::cell::OnceCell;

use rugkit_reactive::{Owner, Signal, signal};
use rugkit_render::Color;
use rugkit_shell::{Appearance, ColorScheme};

use crate::Spring;

/// The design tokens widgets read their defaults from.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Theme {
    /// Window background.
    pub background: Color,
    /// Raised surfaces such as cards.
    pub surface: Color,
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
        ink: Color::from_hex(0x1E1B3A),
        ink_soft: Color::from_hex(0x6B6687),
        accent: Color::from_hex(0x7B61FF),
        positive: Color::from_hex(0x00B894),
        negative: Color::from_hex(0xFF5A5F),
        layout_spring: Spring::BOUNCY,
        color_spring: Spring::SNAPPY,
    };

    /// The default dark theme: deep indigo surfaces with crisp contrast.
    pub const PLAYFUL_DARK: Self = Self {
        background: Color::from_hex(0x141224),
        surface: Color::from_hex(0x1F1C36),
        ink: Color::from_hex(0xF5F3FF),
        ink_soft: Color::from_hex(0xA6A1C4),
        accent: Color::from_hex(0x9B85FF),
        positive: Color::from_hex(0x26D9B2),
        negative: Color::from_hex(0xFF6B70),
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

/// The active theme. Reading this inside a reactive prop subscribes to system appearance changes.
pub fn theme() -> Theme {
    theme_signal().get()
}

/// Updates the active theme to match `appearance` (called automatically by [`crate::Ui`]).
pub fn set_appearance(appearance: Appearance) {
    theme_signal().set_if_changed(Theme::for_appearance(appearance));
}
