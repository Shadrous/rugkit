//! Vector [`Path`] builder (lines, quadratic/cubic curves, shapes and SVG `d` strings).

use vello::kurbo::{BezPath, Cap, Join, Shape, Stroke};

use crate::{Point, Rect};

/// A 2D vector path in logical pixels.
///
/// Build paths with [`Path::new`] and [`move_to`](Self::move_to) / [`line_to`](Self::line_to) /
/// [`quad_to`](Self::quad_to) / [`cubic_to`](Self::cubic_to) / [`close`](Self::close), or parse
/// an SVG path data string with [`Path::from_svg_d`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    inner: BezPath,
}

impl Path {
    /// Creates an empty path.
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a new subpath at `(x, y)`.
    pub fn move_to(&mut self, p: Point) -> &mut Self {
        self.inner.move_to(p.to_kurbo());
        self
    }

    /// Adds a straight line segment to `p`.
    pub fn line_to(&mut self, p: Point) -> &mut Self {
        self.inner.line_to(p.to_kurbo());
        self
    }

    /// Adds a quadratic Bézier curve with control point `ctrl` ending at `to`.
    pub fn quad_to(&mut self, ctrl: Point, to: Point) -> &mut Self {
        self.inner.quad_to(ctrl.to_kurbo(), to.to_kurbo());
        self
    }

    /// Adds a cubic Bézier curve with control points `c1`, `c2` ending at `to`.
    pub fn cubic_to(&mut self, c1: Point, c2: Point, to: Point) -> &mut Self {
        self.inner
            .curve_to(c1.to_kurbo(), c2.to_kurbo(), to.to_kurbo());
        self
    }

    /// Closes the current subpath.
    pub fn close(&mut self) -> &mut Self {
        self.inner.close_path();
        self
    }

    /// Appends a circle centred at `center` with `radius`.
    pub fn circle(&mut self, center: Point, radius: f64) -> &mut Self {
        let c = vello::kurbo::Circle::new(center.to_kurbo(), radius);
        self.inner.extend(c.path_elements(0.1));
        self
    }

    /// Appends a rounded rectangle.
    pub fn rounded_rect(&mut self, rect: Rect, radius: f64) -> &mut Self {
        let rr = vello::kurbo::RoundedRect::from_rect(rect.to_kurbo(), radius);
        self.inner.extend(rr.path_elements(0.1));
        self
    }

    /// Parses an SVG path `d` attribute (supporting `M/m`, `L/l`, `H/h`, `V/v`, `C/c`, `Q/q`, `Z/z`).
    ///
    /// Invalid tokens are skipped gracefully.
    pub fn from_svg_d(d: &str) -> Self {
        let inner = BezPath::from_svg(d).unwrap_or_default();
        Self { inner }
    }

    pub(crate) fn to_kurbo(&self) -> &BezPath {
        &self.inner
    }
}

pub(crate) fn round_stroke(width: f64) -> Stroke {
    Stroke::new(width)
        .with_caps(Cap::Round)
        .with_join(Join::Round)
}
