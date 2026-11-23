//! Geometry in logical pixels.

/// A point in logical pixels.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Point {
    /// Horizontal position.
    pub x: f64,
    /// Vertical position (grows downwards).
    pub y: f64,
}

impl Point {
    /// The origin `(0, 0)`.
    pub const ZERO: Self = Self::new(0.0, 0.0);

    /// Creates a point.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub(crate) fn to_kurbo(self) -> vello::kurbo::Point {
        vello::kurbo::Point::new(self.x, self.y)
    }
}

/// A 2D offset in logical pixels.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Vec2 {
    /// Horizontal component.
    pub x: f64,
    /// Vertical component.
    pub y: f64,
}

impl Vec2 {
    /// The zero offset.
    pub const ZERO: Self = Self::new(0.0, 0.0);

    /// Creates an offset.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// A width and height in logical pixels.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Size {
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

impl Size {
    /// Creates a size.
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

/// An axis-aligned rectangle in logical pixels, stored as its edges.
///
/// ```
/// use rugkit_render::Rect;
///
/// let r = Rect::from_xywh(10.0, 20.0, 100.0, 50.0);
/// assert_eq!(r.width(), 100.0);
/// assert_eq!(r.inset(5.0).height(), 40.0);
/// ```
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Rect {
    /// Left edge.
    pub x0: f64,
    /// Top edge.
    pub y0: f64,
    /// Right edge.
    pub x1: f64,
    /// Bottom edge.
    pub y1: f64,
}

impl Rect {
    /// Creates a rectangle from its edges.
    pub const fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { x0, y0, x1, y1 }
    }

    /// Creates a rectangle from its top-left corner and size.
    pub const fn from_xywh(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self::new(x, y, x + width, y + height)
    }

    /// Creates a rectangle from an origin and a size.
    pub const fn from_origin_size(origin: Point, size: Size) -> Self {
        Self::from_xywh(origin.x, origin.y, size.width, size.height)
    }

    /// Width.
    pub fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    /// Height.
    pub fn height(&self) -> f64 {
        self.y1 - self.y0
    }

    /// Top-left corner.
    pub fn origin(&self) -> Point {
        Point::new(self.x0, self.y0)
    }

    /// Size.
    pub fn size(&self) -> Size {
        Size::new(self.width(), self.height())
    }

    /// Centre point.
    pub fn center(&self) -> Point {
        Point::new((self.x0 + self.x1) * 0.5, (self.y0 + self.y1) * 0.5)
    }

    /// Shrinks the rectangle by `amount` on every side. Negative values grow it.
    pub fn inset(&self, amount: f64) -> Self {
        Self::new(
            self.x0 + amount,
            self.y0 + amount,
            self.x1 - amount,
            self.y1 - amount,
        )
    }

    /// Moves the rectangle by `offset`.
    pub fn translate(&self, offset: Vec2) -> Self {
        Self::new(
            self.x0 + offset.x,
            self.y0 + offset.y,
            self.x1 + offset.x,
            self.y1 + offset.y,
        )
    }

    pub(crate) fn to_kurbo(self) -> vello::kurbo::Rect {
        vello::kurbo::Rect::new(self.x0, self.y0, self.x1, self.y1)
    }
}

/// A 2D affine transform (translation, scale, and so on) in logical pixels.
///
/// Compose transforms with [`Transform::then`]. `a.then(b)` applies `a` first, then `b`.
///
/// ```
/// use rugkit_render::{Point, Transform, Vec2};
///
/// let t = Transform::scale_about(2.0, Point::new(10.0, 10.0)).then(Transform::translate(Vec2::new(5.0, 0.0)));
/// assert_eq!(t.apply(Point::new(10.0, 10.0)), Point::new(15.0, 10.0));
/// ```
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Transform {
    /// Matrix coefficients `[a, b, c, d, e, f]`, mapping `(x, y)` to
    /// `(a·x + c·y + e, b·x + d·y + f)`.
    pub coeffs: [f64; 6],
}

impl Transform {
    /// The transform that changes nothing.
    pub const IDENTITY: Self = Self {
        coeffs: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    };

    /// Moves by `offset`.
    pub const fn translate(offset: Vec2) -> Self {
        Self {
            coeffs: [1.0, 0.0, 0.0, 1.0, offset.x, offset.y],
        }
    }

    /// Scales uniformly about the origin.
    pub const fn scale(s: f64) -> Self {
        Self {
            coeffs: [s, 0.0, 0.0, s, 0.0, 0.0],
        }
    }

    /// Scales uniformly about `center`, which stays in place.
    pub fn scale_about(s: f64, center: Point) -> Self {
        Self {
            coeffs: [s, 0.0, 0.0, s, center.x * (1.0 - s), center.y * (1.0 - s)],
        }
    }

    /// Applies `self` first, then `next`.
    pub fn then(self, next: Self) -> Self {
        let [a1, b1, c1, d1, e1, f1] = self.coeffs;
        let [a2, b2, c2, d2, e2, f2] = next.coeffs;
        Self {
            coeffs: [
                a2 * a1 + c2 * b1,
                b2 * a1 + d2 * b1,
                a2 * c1 + c2 * d1,
                b2 * c1 + d2 * d1,
                a2 * e1 + c2 * f1 + e2,
                b2 * e1 + d2 * f1 + f2,
            ],
        }
    }

    /// Maps a point through the transform.
    pub fn apply(&self, p: Point) -> Point {
        let [a, b, c, d, e, f] = self.coeffs;
        Point::new(a * p.x + c * p.y + e, b * p.x + d * p.y + f)
    }

    pub(crate) fn to_kurbo(self) -> vello::kurbo::Affine {
        vello::kurbo::Affine::new(self.coeffs)
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}
