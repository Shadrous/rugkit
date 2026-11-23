//! Drawing for rugkit.
//!
//! This crate gives rugkit its own stable drawing API. The [`Painter`] draws rounded
//! rectangles, gradients, soft shadows and crisp text in logical pixels. vello (GPU
//! rasterisation) and parley (text shaping and layout) do the work underneath, but none of
//! their types appear in this API.
//!
//! Most users get these types through the `rugkit` facade crate rather than depending on this
//! crate directly.

mod brush;
mod color;
mod geometry;
mod painter;
mod text;

pub use brush::{Brush, Shadow};
pub use color::Color;
pub use geometry::{Point, Rect, Size, Transform, Vec2};
pub use painter::{Canvas, Painter};
pub use text::{FontFamily, FontWeight, TextStyle, TextSystem};

/// Internal integration API for `rugkit-shell`.
///
/// This exposes the rendering backend so the shell can put recorded frames on the GPU.
/// The `rugkit` facade doesn't re-export it, and it has **no stability guarantees**.
#[doc(hidden)]
pub mod backend {
    pub use vello;
}
