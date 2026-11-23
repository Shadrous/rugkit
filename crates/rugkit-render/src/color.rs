//! Colours.

/// A colour in non-premultiplied sRGB, with each channel in `0.0..=1.0`.
///
/// ```
/// use rugkit_render::Color;
///
/// let coral = Color::from_hex(0xFF6B5B);
/// let faded = coral.with_alpha(0.5);
/// assert_eq!(faded.a, 0.5);
/// ```
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Color {
    /// Red, `0.0..=1.0`.
    pub r: f32,
    /// Green, `0.0..=1.0`.
    pub g: f32,
    /// Blue, `0.0..=1.0`.
    pub b: f32,
    /// Alpha (opacity), `0.0..=1.0`.
    pub a: f32,
}

impl Color {
    /// Fully transparent black.
    pub const TRANSPARENT: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);
    /// Opaque black.
    pub const BLACK: Self = Self::rgba(0.0, 0.0, 0.0, 1.0);
    /// Opaque white.
    pub const WHITE: Self = Self::rgba(1.0, 1.0, 1.0, 1.0);

    /// Creates a colour from float channels in `0.0..=1.0`.
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Creates an opaque colour from 8-bit channels.
    pub const fn rgb8(r: u8, g: u8, b: u8) -> Self {
        Self::rgba8(r, g, b, 255)
    }

    /// Creates a colour from 8-bit channels, including alpha.
    pub const fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// Creates an opaque colour from a `0xRRGGBB` value.
    pub const fn from_hex(rgb: u32) -> Self {
        Self::rgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    }

    /// Returns the same colour with alpha replaced by `a`.
    pub const fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// Linearly mixes `self` towards `other` by `t` (`0.0` = `self`, `1.0` = `other`), per channel in sRGB.
    pub fn mix(self, other: Self, t: f32) -> Self {
        let l = |a: f32, b: f32| a + (b - a) * t;
        Self::rgba(
            l(self.r, other.r),
            l(self.g, other.g),
            l(self.b, other.b),
            l(self.a, other.a),
        )
    }

    pub(crate) fn to_peniko(self) -> vello::peniko::Color {
        vello::peniko::Color::new([self.r, self.g, self.b, self.a])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        let c = Color::from_hex(0x336699);
        assert_eq!(c, Color::rgb8(0x33, 0x66, 0x99));
        assert_eq!(c.a, 1.0);
    }

    #[test]
    fn mix_endpoints() {
        let a = Color::BLACK;
        let b = Color::WHITE;
        assert_eq!(a.mix(b, 0.0), a);
        assert_eq!(a.mix(b, 1.0), b);
        assert_eq!(a.mix(b, 0.5).r, 0.5);
    }
}
