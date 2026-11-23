//! Brushes (what a shape is filled with) and shadows.

use crate::{Color, Point, Vec2};

/// What a shape or stroke is painted with.
#[derive(Clone, Debug, PartialEq)]
pub enum Brush {
    /// A single colour.
    Solid(Color),
    /// A linear gradient between two points, in the same coordinates as the shape it fills.
    LinearGradient {
        /// Where offset `0.0` lies.
        start: Point,
        /// Where offset `1.0` lies.
        end: Point,
        /// Colour stops as `(offset, colour)`, offsets in `0.0..=1.0`, in ascending order.
        stops: Vec<(f32, Color)>,
    },
}

impl Brush {
    /// A linear gradient from `start` to `end` through `stops`.
    pub fn linear(start: Point, end: Point, stops: impl Into<Vec<(f32, Color)>>) -> Self {
        Self::LinearGradient {
            start,
            end,
            stops: stops.into(),
        }
    }

    pub(crate) fn to_peniko(&self) -> vello::peniko::Brush {
        match self {
            Self::Solid(c) => vello::peniko::Brush::Solid(c.to_peniko()),
            Self::LinearGradient { start, end, stops } => {
                let stops: Vec<(f32, vello::peniko::Color)> =
                    stops.iter().map(|(o, c)| (*o, c.to_peniko())).collect();
                vello::peniko::Gradient::new_linear(start.to_kurbo(), end.to_kurbo())
                    .with_stops(stops.as_slice())
                    .into()
            }
        }
    }
}

impl From<Color> for Brush {
    fn from(color: Color) -> Self {
        Self::Solid(color)
    }
}

/// A soft drop shadow cast by a rounded rectangle.
///
/// Stack several shadows (a tight dark one plus a wide faint one) for the layered,
/// premium look rugkit aims for.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Shadow {
    /// Shadow colour, usually a dark tint with low alpha.
    pub color: Color,
    /// How far the shadow is shifted from the shape.
    pub offset: Vec2,
    /// Blur radius in logical pixels (roughly where the shadow fades out, like CSS `blur`).
    pub blur: f64,
    /// Grows (positive) or shrinks (negative) the shadow shape before blurring.
    pub spread: f64,
}

impl Shadow {
    /// Creates a shadow with no spread.
    pub const fn new(color: Color, offset: Vec2, blur: f64) -> Self {
        Self {
            color,
            offset,
            blur,
            spread: 0.0,
        }
    }

    /// Returns the shadow with `spread` set.
    pub const fn with_spread(self, spread: f64) -> Self {
        Self { spread, ..self }
    }
}
