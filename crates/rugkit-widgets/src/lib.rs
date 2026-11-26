//! The rugkit view tree, implicit spring animations, theming, inspector and built-in widgets.
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

mod canvas;
mod common;
mod controls;
mod harness;
#[cfg(feature = "icons")]
pub mod icons;
mod inspector;
pub mod motion;
mod prop;
pub mod snapshot;
mod table;
mod theme;
mod tree;
mod ui;
mod view;
mod widgets;

pub use canvas::{CanvasView, HitRegions, canvas};
pub use controls::{
    Checkbox, Modal, Popover, Radio, Scroll, Select, Slider, Tabs, Toggle, Tooltip, VirtualList,
    checkbox, modal, popover, radio, scroll, select, slider, tabs, toggle, tooltip, virtual_list,
};
pub use harness::Harness;
#[cfg(feature = "icons")]
pub use icons::{Icon, IconName, icon};
pub use motion::{Animatable, Animated, Motion, Spring};
pub use prop::{IntoProp, Prop};
pub use rugkit_layout::{Align, Direction, Insets, Justify};
pub use rugkit_shell::{A11yAction, A11yId, A11yNode, A11yTree, Role};
pub use snapshot::{SnapshotOutcome, check_snapshot};
pub use table::{CellAlign, Column, SelectionMode, SortDirection, Table, table};
pub use theme::{Radii, Spacing, Theme, TypeScale, set_appearance, set_theme, theme};
pub use tree::NodeId;
pub use ui::Ui;
pub use view::{
    AnyView, BuildCx, Children, DirectionScope, ThemeScope, View, direction_scope, theme_scope,
};
pub use widgets::{
    Button, Image, Stack, Text, TextInput, button, column, grid, image, row, text, text_input,
    zstack,
};
