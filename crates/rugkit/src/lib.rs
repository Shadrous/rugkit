//! # rugkit
//!
//! *Rugkit - The one framework that cares about visuals.*
//!
//! rugkit is a Rust GUI framework built around visual polish and motion. This is the facade
//! crate: the one dependency you add. It re-exports everything you need.
//!
//! - **Views** are plain Rust builder chains ([`text`], [`button`], [`row`], [`column()`]),
//!   built once into a retained tree.
//! - **State** lives in Copy [`Signal`]s. Closures that read signals keep exactly the
//!   affected nodes up to date. Nothing is diffed.
//! - **Motion is implicit.** Every visual change (size, position, colour, elevation, scale)
//!   springs to its new value and keeps its velocity when interrupted. The OS "Reduce motion"
//!   setting is respected.
//!
//! ```no_run
//! use rugkit::prelude::*;
//!
//! fn main() -> Result<(), rugkit::Error> {
//!     rugkit::launch(WindowOptions::default(), || {
//!         let count = signal(0);
//!         column((
//!             text(move || count.get().to_string()).size(64.0).weight(FontWeight::BLACK),
//!             button("Add one").on_click(move || count.update(|c| *c += 1)),
//!         ))
//!         .gap(16.0)
//!     })
//! }
//! ```
//!
//! For full control over drawing, implement [`App`] and use the [`Painter`] directly via
//! [`run`].
//!
//! **Status: M1.** See `PLAN.md` in the repository for the roadmap.

pub use rugkit_reactive::{
    Effect, Memo, Owner, Signal, batch, effect, memo, on_cleanup, signal, untrack,
};
pub use rugkit_render::{
    Brush, Color, FontFamily, FontWeight, Painter, Point, Rect, Shadow, Size, TextStyle, Transform,
    Vec2,
};
pub use rugkit_shell::{
    App, Error, FrameInfo, FrameStats, InputEvent, PointerButton, WindowOptions, run,
};
pub use rugkit_widgets::{
    Align, AnyView, Button, Children, Insets, IntoProp, Justify, Prop, Spring, Stack, Text, Theme,
    Ui, View, button, column, row, text, theme,
};

/// Opens a window showing the view returned by `view`, centred, and runs until it's closed.
///
/// `view` runs once, on the UI thread, inside a reactive owner. Create your signals inside it.
pub fn launch<V: View>(options: WindowOptions, view: impl FnOnce() -> V) -> Result<(), Error> {
    run(options, Ui::new(view))
}

/// Everything most apps need, in one import: `use rugkit::prelude::*;`.
pub mod prelude {
    pub use crate::{
        Align, App, Brush, Color, FontFamily, FontWeight, FrameInfo, FrameStats, Insets, Justify,
        Painter, Point, Rect, Shadow, Signal, Size, Spring, TextStyle, Vec2, View, WindowOptions,
        batch, button, column, effect, memo, row, signal, text, theme,
    };
}
