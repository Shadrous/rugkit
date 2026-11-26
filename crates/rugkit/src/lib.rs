//! # rugkit
//!
//! *Rugkit - The one framework that cares about visuals.*
//!
//! rugkit is a Rust GUI framework built around visual polish and motion. This is the facade
//! crate: the one dependency you add. It re-exports everything you need.
//!
//! - **Views** are plain Rust builder chains ([`text`], [`button`], [`text_input`], [`checkbox`],
//!   [`toggle`], [`slider`], [`radio`], [`select`], [`tabs`], [`scroll`], [`virtual_list`],
//!   [`modal`], [`popover`], [`tooltip`], [`row`], [`column()`], [`grid`], [`zstack`]), built
//!   once into a retained tree.
//! - **State** lives in Copy [`Signal`]s. Closures that read signals keep exactly the
//!   affected nodes up to date. Nothing is diffed.
//! - **Motion is implicit.** Every visual change (size, position, colour, elevation, scale,
//!   opacity, focus ring) springs to its new value and keeps its velocity when interrupted.
//!   The OS "Reduce motion" setting is respected.
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
//! [`run`] or [`run_windows`].
//!
//! **Status: M4.** See `PLAN.md` in the repository for the roadmap.

pub use rugkit_reactive::{
    Effect, Memo, Owner, Signal, Tracker, batch, effect, memo, on_cleanup, signal, untrack,
};
pub use rugkit_render::{
    Brush, Color, FontFamily, FontWeight, ImageData, OffscreenRenderer, Painter, Path, Point, Rect,
    Shadow, Size, SnapshotDiff, TextStyle, Transform, Vec2, compare_rgba,
};
pub use rugkit_shell::{
    A11yAction, A11yId, A11yNode, A11yTree, App, Appearance, ColorScheme, Error, FrameInfo,
    FrameStats, ImeEvent, ImeState, InputEvent, Key, MenuBar, MenuEntry, MenuItem, Modifiers,
    PointerButton, Role, Shortcut, StandardMenuItem, Submenu, WakeHandle, WindowCommand,
    WindowOptions, WindowSpec, clipboard, run, run_windows,
};
pub use rugkit_widgets::{
    Align, Animated, AnyView, Button, CanvasView, CellAlign, Checkbox, Children, Column, Direction,
    DirectionScope, Harness, HitRegions, Image, Insets, IntoProp, Justify, Modal, Motion, Popover,
    Prop, Radii, Radio, Scroll, Select, SelectionMode, Slider, SnapshotOutcome, SortDirection,
    Spacing, Spring, Stack, Table, Tabs, Text, TextInput, Theme, ThemeScope, Toggle, Tooltip,
    TypeScale, Ui, View, VirtualList, button, canvas, check_snapshot, checkbox, column,
    direction_scope, grid, image, modal, popover, radio, row, scroll, select, set_appearance,
    set_theme, slider, table, tabs, text, text_input, theme, theme_scope, toggle, tooltip,
    virtual_list, zstack,
};
#[cfg(feature = "icons")]
pub use rugkit_widgets::{Icon, IconName, icon};

/// Opens a window showing the view returned by `view`, centred, and runs until it's closed.
///
/// `view` runs once, on the UI thread, inside a reactive owner. Create your signals inside it.
pub fn launch<V: View>(options: WindowOptions, view: impl FnOnce() -> V) -> Result<(), Error> {
    run(options, Ui::new(view))
}

/// Everything most apps need, in one import: `use rugkit::prelude::*;`.
pub mod prelude {
    pub use crate::{
        Align, App, Appearance, Brush, CellAlign, Color, ColorScheme, Column, Direction,
        FontFamily, FontWeight, FrameInfo, FrameStats, HitRegions, ImageData, Insets, Justify, Key,
        MenuBar, MenuItem, Modifiers, Painter, Path, Point, Rect, Role, SelectionMode, Shadow,
        Shortcut, Signal, Size, SortDirection, Spring, StandardMenuItem, Submenu, TextStyle, Theme,
        Vec2, View, WindowOptions, WindowSpec, batch, button, canvas, checkbox, column,
        direction_scope, effect, grid, image, memo, modal, popover, radio, row, scroll, select,
        set_theme, signal, slider, table, tabs, text, text_input, theme, theme_scope, toggle,
        tooltip, virtual_list, zstack,
    };
    #[cfg(feature = "icons")]
    pub use crate::{IconName, icon};
}
