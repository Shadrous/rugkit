//! OS appearance: light/dark mode and system accent colour.

use rugkit_render::Color;

/// System light or dark colour scheme.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum ColorScheme {
    /// Light mode.
    #[default]
    Light,
    /// Dark mode.
    Dark,
}

/// OS appearance settings observed by the shell.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Appearance {
    /// Light or dark mode.
    pub scheme: ColorScheme,
    /// System accent colour (e.g. macOS Settings › Appearance › Accent colour), if available.
    pub accent: Option<Color>,
}

impl Appearance {
    /// Creates an [`Appearance`] with the given scheme and optional accent colour.
    pub const fn new(scheme: ColorScheme, accent: Option<Color>) -> Self {
        Self { scheme, accent }
    }

    /// True if the system is in dark mode.
    pub const fn is_dark(self) -> bool {
        matches!(self.scheme, ColorScheme::Dark)
    }
}
