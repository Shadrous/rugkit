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
