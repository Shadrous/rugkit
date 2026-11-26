//! Input event types: pointer, keyboard, modifiers, IME, focus and menu actions.

use rugkit_render::Point;

use crate::appearance::Appearance;

/// A mouse or trackpad button.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PointerButton {
    /// Primary (left) button or a trackpad click.
    Primary,
    /// Secondary (right) button.
    Secondary,
    /// Middle button.
    Middle,
}

/// Active keyboard modifier keys.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    /// Shift key.
    pub shift: bool,
    /// Control key.
    pub ctrl: bool,
    /// Option (macOS) / Alt (other platforms).
    pub alt: bool,
    /// Command (macOS) / Super / Windows key.
    pub meta: bool,
}

impl Modifiers {
    /// The platform's primary shortcut modifier: Command (`meta`) on macOS, Control elsewhere.
    pub fn command(self) -> bool {
        if cfg!(target_os = "macos") {
            self.meta
        } else {
            self.ctrl
        }
    }

    /// True if no modifier keys are held.
    pub fn is_empty(self) -> bool {
        !self.shift && !self.ctrl && !self.alt && !self.meta
    }
}

/// A logical keyboard key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// A character key (e.g. `"a"`, `"A"`, `"+"`, `"1"`).
    Character(String),
    /// Tab key.
    Tab,
    /// Return / Enter key.
    Enter,
    /// Escape key.
    Escape,
    /// Backspace / Delete-backward key.
    Backspace,
    /// Forward Delete key.
    Delete,
    /// Space bar.
    Space,
    /// Left arrow.
    ArrowLeft,
    /// Right arrow.
    ArrowRight,
    /// Up arrow.
    ArrowUp,
    /// Down arrow.
    ArrowDown,
    /// Home key.
    Home,
    /// End key.
    End,
    /// Page Up key.
    PageUp,
    /// Page Down key.
    PageDown,
    /// Any other named key.
    Named(String),
}

/// Input Method Editor (IME) composition events for CJK, dead keys and emoji pickers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImeEvent {
    /// IME composition was enabled for the window.
    Enabled,
    /// Uncommitted composition text, with an optional active clause byte range.
    Preedit(String, Option<(usize, usize)>),
    /// Finalised text to insert at the caret.
    Commit(String),
    /// IME composition was disabled or cancelled.
    Disabled,
}

/// Input delivered to [`crate::App::event`]. Positions are logical pixels in window coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum InputEvent {
    /// The pointer moved.
    PointerMove(Point),
    /// A button was pressed.
    PointerDown(Point, PointerButton),
    /// A button was released.
    PointerUp(Point, PointerButton),
    /// The pointer left the window.
    PointerLeave,
    /// A key was pressed.
    KeyDown {
        /// Logical key.
        key: Key,
        /// Active modifiers.
        modifiers: Modifiers,
        /// Printable text produced by this key press, if any.
        text: Option<String>,
        /// Whether this is an OS key-repeat event.
        repeat: bool,
    },
    /// A key was released.
    KeyUp {
        /// Logical key.
        key: Key,
        /// Active modifiers.
        modifiers: Modifiers,
    },
    /// An IME composition event (CJK, dead keys, emoji).
    Ime(ImeEvent),
    /// The window gained (`true`) or lost (`false`) keyboard focus.
    Focused(bool),
    /// The OS appearance (light/dark or accent colour) changed.
    AppearanceChanged(Appearance),
    /// A native menu bar or context menu item was activated, by its item ID.
    MenuAction(String),
}
