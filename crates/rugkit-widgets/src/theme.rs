//! Design tokens: the first draft of rugkit's bold, playful design language.
//!
//! M1 ships one fixed light theme. Reactive, overridable tokens arrive in M3.

use rugkit_render::Color;

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
    /// Brand accent: primary buttons and highlights.
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
    /// The default theme: warm, saturated and bouncy.
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
}

impl Default for Theme {
    fn default() -> Self {
        Self::PLAYFUL_LIGHT
    }
}

/// The active theme.
pub fn theme() -> Theme {
    Theme::PLAYFUL_LIGHT
}
