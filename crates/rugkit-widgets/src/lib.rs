//! The rugkit view tree, implicit spring animations and built-in widgets.
//!
//! Views are plain Rust builder chains. They're built **once** into a retained tree. After
//! that, reactive properties (closures that read signals) update exactly the nodes they
//! belong to, and every visual change springs to its new value automatically:
//!
//! ```no_run
//! use rugkit_reactive::signal;
//! use rugkit_widgets::{button, column, text};
//!
//! let count = signal(0);
//! let view = column((
//!     text(move || count.get().to_string()).size(64.0),
//!     button("Add one").on_click(move || count.update(|c| *c += 1)),
//! ))
//! .gap(16.0);
//! ```
//!
//! [`Ui`] hosts a tree in a window. Most apps go through the `rugkit` facade's `launch`.

mod common;
pub mod motion;
mod prop;
mod theme;
mod tree;
mod ui;
mod view;
mod widgets;

pub use motion::{Animatable, Animated, Motion, Spring};
pub use prop::{IntoProp, Prop};
pub use rugkit_layout::{Align, Insets, Justify};
pub use theme::{Theme, theme};
pub use tree::NodeId;
pub use ui::Ui;
pub use view::{AnyView, BuildCx, Children, View};
pub use widgets::{Button, Stack, Text, button, column, row, text};
