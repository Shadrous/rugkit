//! Native menu bars and context menus, backed by `muda` internally.

use crate::input::Modifiers;

/// A keyboard shortcut attached to a [`MenuItem`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortcut {
    /// Required modifiers.
    pub modifiers: Modifiers,
    /// Key character or name (e.g. `"+"`, `"-"`, `"0"`, `"r"`, `"q"`).
    pub key: String,
}

impl Shortcut {
    /// Creates a shortcut using the platform's primary command modifier (⌘ on macOS, Ctrl elsewhere).
    pub fn cmd(key: impl Into<String>) -> Self {
        let mut modifiers = Modifiers::default();
        if cfg!(target_os = "macos") {
            modifiers.meta = true;
        } else {
            modifiers.ctrl = true;
        }
        Self {
            modifiers,
            key: key.into(),
        }
    }

    /// Creates a shortcut with explicit modifiers and key.
    pub fn new(modifiers: Modifiers, key: impl Into<String>) -> Self {
        Self {
            modifiers,
            key: key.into(),
        }
    }
}

/// A clickable item in a native menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    /// Identifier delivered in [`crate::InputEvent::MenuAction`] when clicked.
    pub id: String,
    /// Visible label.
    pub label: String,
    /// Whether the item is enabled.
    pub enabled: bool,
    /// Optional keyboard accelerator shown next to the label.
    pub shortcut: Option<Shortcut>,
}

impl MenuItem {
    /// Creates an enabled menu item with `id` and `label`.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            enabled: true,
            shortcut: None,
        }
    }

    /// Attaches a keyboard shortcut.
    pub fn shortcut(mut self, shortcut: Shortcut) -> Self {
        self.shortcut = Some(shortcut);
        self
    }

    /// Sets whether the item is enabled.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Standard OS-handled menu items (About, Quit, Cut/Copy/Paste, etc.).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StandardMenuItem {
    /// "About" item for the application.
    About,
    /// "Hide" item for the application.
    Hide,
    /// "Hide Others"
    HideOthers,
    /// "Show All"
    ShowAll,
    /// "Quit" item for the application.
    Quit,
    /// Cut
    Cut,
    /// Copy
    Copy,
    /// Paste
    Paste,
    /// Select All
    SelectAll,
    /// Minimize window
    Minimize,
    /// Close window
    CloseWindow,
}

/// One entry inside a [`Submenu`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuEntry {
    /// A custom application item.
    Item(MenuItem),
    /// A standard OS menu item.
    Standard(StandardMenuItem),
    /// A horizontal separator line.
    Separator,
    /// A nested submenu.
    Submenu(Submenu),
}

/// A named menu containing entries (appears as a top-level menu in a [`MenuBar`] or nested).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Submenu {
    /// Menu title.
    pub title: String,
    /// Entries in display order.
    pub entries: Vec<MenuEntry>,
}

impl Submenu {
    /// Creates an empty submenu with `title`.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            entries: Vec::new(),
        }
    }

    /// Appends a custom [`MenuItem`].
    pub fn item(mut self, item: MenuItem) -> Self {
        self.entries.push(MenuEntry::Item(item));
        self
    }

    /// Appends a [`StandardMenuItem`].
    pub fn standard(mut self, item: StandardMenuItem) -> Self {
        self.entries.push(MenuEntry::Standard(item));
        self
    }

    /// Appends a separator line.
    pub fn separator(mut self) -> Self {
        self.entries.push(MenuEntry::Separator);
        self
    }

    /// Appends a nested submenu.
    pub fn submenu(mut self, submenu: Submenu) -> Self {
        self.entries.push(MenuEntry::Submenu(submenu));
        self
    }
}

/// A native application menu bar.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuBar {
    /// Top-level menus from left to right.
    pub menus: Vec<Submenu>,
}

impl MenuBar {
    /// Creates an empty menu bar.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a top-level submenu.
    pub fn submenu(mut self, submenu: Submenu) -> Self {
        self.menus.push(submenu);
        self
    }

    /// A sensible default macOS-style menu bar (App menu with Quit, Edit menu with Cut/Copy/Paste).
    pub fn standard_app(app_name: impl Into<String>) -> Self {
        let app_name = app_name.into();
        Self::new()
            .submenu(
                Submenu::new(app_name)
                    .standard(StandardMenuItem::About)
                    .separator()
                    .standard(StandardMenuItem::Hide)
                    .standard(StandardMenuItem::HideOthers)
                    .standard(StandardMenuItem::ShowAll)
                    .separator()
                    .standard(StandardMenuItem::Quit),
            )
            .submenu(
                Submenu::new("Edit")
                    .standard(StandardMenuItem::Cut)
                    .standard(StandardMenuItem::Copy)
                    .standard(StandardMenuItem::Paste)
                    .standard(StandardMenuItem::SelectAll),
            )
    }
}

// ---- muda conversion (crate-private) ----------------------------------------------------

pub(crate) fn install_menu_bar(bar: &MenuBar) -> Option<muda::Menu> {
    let menu = muda::Menu::new();
    for sub in &bar.menus {
        let built = build_submenu(sub);
        let _ = menu.append(&built);
    }
    #[cfg(target_os = "macos")]
    menu.init_for_nsapp();
    Some(menu)
}

fn build_submenu(sub: &Submenu) -> muda::Submenu {
    let out = muda::Submenu::new(&sub.title, true);
    for entry in &sub.entries {
        match entry {
            MenuEntry::Item(item) => {
                let accel = item.shortcut.as_ref().and_then(to_accelerator);
                let built = muda::MenuItem::with_id(&item.id, &item.label, item.enabled, accel);
                let _ = out.append(&built);
            }
            MenuEntry::Standard(std_item) => {
                let built = match std_item {
                    StandardMenuItem::About => muda::PredefinedMenuItem::about(None, None),
                    StandardMenuItem::Hide => muda::PredefinedMenuItem::hide(None),
                    StandardMenuItem::HideOthers => muda::PredefinedMenuItem::hide_others(None),
                    StandardMenuItem::ShowAll => muda::PredefinedMenuItem::show_all(None),
                    StandardMenuItem::Quit => muda::PredefinedMenuItem::quit(None),
                    StandardMenuItem::Cut => muda::PredefinedMenuItem::cut(None),
                    StandardMenuItem::Copy => muda::PredefinedMenuItem::copy(None),
                    StandardMenuItem::Paste => muda::PredefinedMenuItem::paste(None),
                    StandardMenuItem::SelectAll => muda::PredefinedMenuItem::select_all(None),
                    StandardMenuItem::Minimize => muda::PredefinedMenuItem::minimize(None),
                    StandardMenuItem::CloseWindow => muda::PredefinedMenuItem::close_window(None),
                };
                let _ = out.append(&built);
            }
            MenuEntry::Separator => {
                let _ = out.append(&muda::PredefinedMenuItem::separator());
            }
            MenuEntry::Submenu(nested) => {
                let built = build_submenu(nested);
                let _ = out.append(&built);
            }
        }
    }
    out
}

fn to_accelerator(s: &Shortcut) -> Option<muda::accelerator::Accelerator> {
    use muda::accelerator::{Accelerator, Code, Modifiers as M};
    let mut mods = M::empty();
    if s.modifiers.shift {
        mods |= M::SHIFT;
    }
    if s.modifiers.ctrl {
        mods |= M::CONTROL;
    }
    if s.modifiers.alt {
        mods |= M::ALT;
    }
    if s.modifiers.meta {
        mods |= M::META;
    }
    let code = match s.key.to_ascii_lowercase().as_str() {
        "a" => Code::KeyA,
        "b" => Code::KeyB,
        "c" => Code::KeyC,
        "d" => Code::KeyD,
        "e" => Code::KeyE,
        "f" => Code::KeyF,
        "g" => Code::KeyG,
        "h" => Code::KeyH,
        "i" => Code::KeyI,
        "j" => Code::KeyJ,
        "k" => Code::KeyK,
        "l" => Code::KeyL,
        "m" => Code::KeyM,
        "n" => Code::KeyN,
        "o" => Code::KeyO,
        "p" => Code::KeyP,
        "q" => Code::KeyQ,
        "r" => Code::KeyR,
        "s" => Code::KeyS,
        "t" => Code::KeyT,
        "u" => Code::KeyU,
        "v" => Code::KeyV,
        "w" => Code::KeyW,
        "x" => Code::KeyX,
        "y" => Code::KeyY,
        "z" => Code::KeyZ,
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        "+" | "=" => Code::Equal,
        "-" => Code::Minus,
        _ => return None,
    };
    Some(Accelerator::new(mods, code))
}
