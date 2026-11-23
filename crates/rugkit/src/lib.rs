//! # rugkit
//!
//! *Rugkit - The one framework that cares about visuals.*
//!
//! rugkit is a Rust GUI framework built around visual polish and motion. This is the facade
//! crate: the one dependency you add. It re-exports everything you need.
//!
//! **Status: M0 (pixels on screen).** So far you get a window and a [`Painter`] that draws
//! rounded rectangles, gradients, soft shadows and crisp text. Signals, layout, widgets and
//! springs arrive in M1. See `PLAN.md` in the repository for the roadmap.
//!
//! ```no_run
//! use rugkit::prelude::*;
//!
//! struct Hello;
//!
//! impl App for Hello {
//!     fn draw(&mut self, p: &mut Painter<'_>, frame: &FrameInfo) {
//!         let card = Rect::from_xywh(40.0, 40.0, 320.0, 160.0);
//!         p.shadow(card, 24.0, &Shadow::new(Color::BLACK.with_alpha(0.15), Vec2::new(0.0, 12.0), 32.0));
//!         p.fill_rect(card, 24.0, Color::WHITE);
//!         p.text("Hello, rugkit", Point::new(64.0, 64.0), &TextStyle::new(28.0).weight(FontWeight::BOLD));
//!     }
//! }
//!
//! fn main() -> Result<(), rugkit::Error> {
//!     rugkit::run(WindowOptions::default(), Hello)
//! }
//! ```

pub use rugkit_render::{
    Brush, Color, FontFamily, FontWeight, Painter, Point, Rect, Shadow, Size, TextStyle, Vec2,
};
pub use rugkit_shell::{App, Error, FrameInfo, FrameStats, WindowOptions, run};

/// Everything most apps need, in one import: `use rugkit::prelude::*;`.
pub mod prelude {
    pub use crate::{
        App, Brush, Color, FontFamily, FontWeight, FrameInfo, FrameStats, Painter, Point, Rect,
        Shadow, Size, TextStyle, Vec2, WindowOptions,
    };
}
