//! Headless interaction test harness for rugkit views.
//!
//! [`Harness`] runs a [`Ui`] without opening a window, finds widgets by [`Role`] or label
//! through the accessibility tree, simulates pointer clicks, keyboard input, IME composition
//! and VoiceOver/AccessKit actions, and uses an isolated in-memory clipboard.
//!
//! ```
//! use rugkit_reactive::signal;
//! use rugkit_widgets::{Harness, Role, button, column, text};
//!
//! let count = signal(0);
//! let mut app = Harness::new(move || {
//!     column((
//!         text(move || format!("Count: {}", count.get())),
//!         button("+").on_click(move || count.update(|c| *c += 1)),
//!     ))
//! });
//! app.click_label("+");
//! assert!(app.find(Role::Label, "Count: 1").is_some());
//! ```

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use rugkit_render::{Canvas, Point, Size};
use rugkit_shell::{
    A11yAction, A11yId, A11yNode, A11yTree, App, Appearance, FrameInfo, FrameStats, ImeEvent,
    ImeState, InputEvent, Key, Modifiers, PointerButton, Role, WindowCommand,
};

use crate::ui::Ui;
use crate::view::View;

/// Drives a [`Ui`] headlessly for automated tests.
pub struct Harness {
    ui: Ui,
    canvas: Canvas,
    size: Size,
    scale: f64,
    time: Duration,
    reduce_motion: bool,
    appearance: Appearance,
    clipboard: Rc<RefCell<String>>,
}

impl std::fmt::Debug for Harness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Harness")
            .field("size", &self.size)
            .field("appearance", &self.appearance)
            .finish_non_exhaustive()
    }
}

impl Harness {
    /// Creates a harness at 800×600 logical pixels (2× Retina scale) and settles the initial frame.
    pub fn new<V: View>(view: impl FnOnce() -> V) -> Self {
        Self::with_size(Size::new(800.0, 600.0), view)
    }

    /// Creates a harness with a custom logical window size and settles the initial frame.
    pub fn with_size<V: View>(size: Size, view: impl FnOnce() -> V) -> Self {
        let clipboard = Rc::new(RefCell::new(String::new()));
        let ui = Ui::new(view).with_memory_clipboard(clipboard.clone());
        let mut h = Self {
            ui,
            canvas: Canvas::new(),
            size,
            scale: 2.0,
            time: Duration::ZERO,
            reduce_motion: false,
            appearance: Appearance::default(),
            clipboard,
        };
        h.settle();
        h
    }

    /// Wraps an already-configured [`Ui`] (e.g. one with [`Ui::on_menu_action`]).
    pub fn from_ui(ui: Ui, size: Size) -> Self {
        let clipboard = Rc::new(RefCell::new(String::new()));
        let mut h = Self {
            ui: ui.with_memory_clipboard(clipboard.clone()),
            canvas: Canvas::new(),
            size,
            scale: 2.0,
            time: Duration::ZERO,
            reduce_motion: false,
            appearance: Appearance::default(),
            clipboard,
        };
        h.settle();
        h
    }

    /// Draws one frame advancing the clock by `dt` seconds.
    pub fn step(&mut self, dt: f64) {
        let dt_dur = Duration::from_secs_f64(dt);
        self.time += dt_dur;
        let info = FrameInfo {
            size: self.size,
            scale: self.scale,
            time: self.time,
            dt: dt_dur,
            stats: FrameStats::default(),
            reduce_motion: self.reduce_motion,
            appearance: self.appearance,
        };
        let mut painter = self.canvas.begin_frame(self.scale);
        self.ui.draw(&mut painter, &info);
    }

    /// Draws frames at 120Hz until every animation settles; returns the frame count.
    pub fn settle(&mut self) -> usize {
        let mut frames = 0;
        loop {
            self.step(1.0 / 120.0);
            frames += 1;
            if !self.ui.wants_frame() {
                return frames;
            }
            assert!(frames < 2000, "rugkit harness: animations never settled");
        }
    }

    /// Whether the UI currently requests another frame.
    pub fn wants_frame(&self) -> bool {
        self.ui.wants_frame()
    }

    /// Returns a fresh snapshot of the accessibility tree.
    pub fn a11y_tree(&mut self) -> A11yTree {
        self.ui
            .a11y_tree()
            .expect("Ui always provides an accessibility tree")
    }

    /// All accessibility nodes with the given [`Role`].
    pub fn by_role(&mut self, role: Role) -> Vec<A11yNode> {
        self.a11y_tree()
            .nodes
            .into_iter()
            .filter(|n| n.role == role)
            .collect()
    }

    /// The first accessibility node whose label equals `label`.
    pub fn by_label(&mut self, label: &str) -> Option<A11yNode> {
        self.a11y_tree()
            .nodes
            .into_iter()
            .find(|n| n.label.as_deref() == Some(label))
    }

    /// The first accessibility node matching both `role` and `label`.
    pub fn find(&mut self, role: Role, label: &str) -> Option<A11yNode> {
        self.a11y_tree()
            .nodes
            .into_iter()
            .find(|n| n.role == role && n.label.as_deref() == Some(label))
    }

    /// The currently focused accessibility node, if any.
    pub fn focused(&mut self) -> Option<A11yNode> {
        let tree = self.a11y_tree();
        let focus = tree.focus;
        if focus == tree.root {
            return None;
        }
        tree.nodes.into_iter().find(|n| n.id == focus)
    }

    /// Delivers a raw [`InputEvent`] and runs one frame.
    pub fn send(&mut self, event: InputEvent) {
        self.ui.event(&event);
        self.step(1.0 / 120.0);
    }

    /// Clicks the centre of the widget at `point`.
    pub fn click_point(&mut self, point: Point) {
        self.ui.event(&InputEvent::PointerMove(point));
        self.ui
            .event(&InputEvent::PointerDown(point, PointerButton::Primary));
        self.ui
            .event(&InputEvent::PointerUp(point, PointerButton::Primary));
        self.step(1.0 / 120.0);
    }

    /// Finds a node by accessibility `label` and clicks its centre. Panics if not found.
    pub fn click_label(&mut self, label: &str) {
        let node = self
            .by_label(label)
            .unwrap_or_else(|| panic!("rugkit harness: no widget with label {label:?}"));
        self.click_point(node.bounds.center());
    }

    /// Presses and releases `key` with no modifiers.
    pub fn press_key(&mut self, key: Key) {
        self.press_key_with(key, Modifiers::default());
    }

    /// Presses and releases `key` with `modifiers`.
    pub fn press_key_with(&mut self, key: Key, modifiers: Modifiers) {
        let text = match &key {
            Key::Character(s) if !modifiers.ctrl && !modifiers.meta && !modifiers.alt => {
                Some(s.clone())
            }
            Key::Space if !modifiers.ctrl && !modifiers.meta && !modifiers.alt => {
                Some(" ".to_owned())
            }
            _ => None,
        };
        self.ui.event(&InputEvent::KeyDown {
            key: key.clone(),
            modifiers,
            text,
            repeat: false,
        });
        self.ui.event(&InputEvent::KeyUp { key, modifiers });
        self.step(1.0 / 120.0);
    }

    /// Types each character of `text` as a key press.
    pub fn type_text(&mut self, text: &str) {
        for ch in text.chars() {
            let s = ch.to_string();
            self.press_key(Key::Character(s));
        }
    }

    /// Sends an IME preedit (uncommitted composition text).
    pub fn ime_preedit(&mut self, text: &str, clause: Option<(usize, usize)>) {
        self.send(InputEvent::Ime(ImeEvent::Preedit(text.to_owned(), clause)));
    }

    /// Sends an IME commit (finalised composition text, e.g. CJK kanji/hanzi).
    pub fn ime_commit(&mut self, text: &str) {
        self.send(InputEvent::Ime(ImeEvent::Commit(text.to_owned())));
    }

    /// Dispatches an accessibility action (the same path VoiceOver uses).
    pub fn a11y_action(&mut self, action: A11yAction) {
        self.ui.a11y_action(&action);
        self.step(1.0 / 120.0);
    }

    /// Activates the node with accessibility `id` via [`A11yAction::Click`].
    pub fn a11y_click(&mut self, id: A11yId) {
        self.a11y_action(A11yAction::Click(id));
    }

    /// Reports the current IME state (enabled flag and caret rectangle).
    pub fn ime_state(&self) -> Option<ImeState> {
        self.ui.ime_state()
    }

    /// Drains any pending [`WindowCommand`]s emitted by the UI.
    pub fn take_commands(&mut self) -> Vec<WindowCommand> {
        self.ui.take_commands()
    }

    /// Changes the simulated OS appearance (light/dark + accent colour).
    pub fn set_appearance(&mut self, appearance: Appearance) {
        self.appearance = appearance;
        self.send(InputEvent::AppearanceChanged(appearance));
    }

    /// Toggles simulated OS Reduce Motion.
    pub fn set_reduce_motion(&mut self, reduce: bool) {
        self.reduce_motion = reduce;
    }

    /// Reads the harness's isolated in-memory clipboard.
    pub fn clipboard_text(&self) -> String {
        self.clipboard.borrow().clone()
    }

    /// Sets the harness's isolated in-memory clipboard.
    pub fn set_clipboard_text(&self, text: &str) {
        *self.clipboard.borrow_mut() = text.to_owned();
    }

    /// Simulates clicking a native menu bar item by ID.
    pub fn trigger_menu(&mut self, id: &str) {
        self.send(InputEvent::MenuAction(id.to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{button, column, row, text, text_input, theme};
    use rugkit_reactive::signal;
    use rugkit_render::Color;
    use rugkit_shell::ColorScheme;

    fn cmd() -> Modifiers {
        let mut m = Modifiers::default();
        if cfg!(target_os = "macos") {
            m.meta = true;
        } else {
            m.ctrl = true;
        }
        m
    }

    #[test]
    fn voiceover_and_keyboard_can_operate_counter() {
        let count = signal(0);
        let mut h = Harness::new(move || {
            column((
                text(move || count.get().to_string()),
                row((
                    button("−").on_click(move || count.update(|c| *c -= 1)),
                    button("Reset").on_click(move || count.set(0)),
                    button("+").on_click(move || count.update(|c| *c += 1)),
                )),
            ))
        });

        // VoiceOver discovers all 3 buttons by role and label.
        let buttons = h.by_role(Role::Button);
        assert_eq!(buttons.len(), 3);
        let plus = h.find(Role::Button, "+").expect("+ button in a11y tree");

        // VoiceOver clicks "+".
        h.a11y_click(plus.id);
        assert_eq!(count.get_untracked(), 1);
        assert!(h.find(Role::Label, "1").is_some());

        // Keyboard Tab wraps from "+" to "−", and Space activates "−".
        h.press_key(Key::Tab);
        assert_eq!(h.focused().and_then(|n| n.label).as_deref(), Some("−"));
        h.press_key(Key::Space);
        assert_eq!(count.get_untracked(), 0);

        // Shift+Tab cycles backwards to "+", and Enter activates it.
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        h.press_key_with(Key::Tab, shift);
        assert_eq!(h.focused().and_then(|n| n.label).as_deref(), Some("+"));
        h.press_key(Key::Enter);
        assert_eq!(count.get_untracked(), 1);
    }

    #[test]
    fn text_input_supports_cjk_ime_editing_and_clipboard() {
        let note = signal(String::new());
        let submitted = signal(String::new());
        let mut h = Harness::new(move || {
            text_input(note)
                .placeholder("Note")
                .on_submit(move |v| submitted.set(v))
        });

        // Before focus, IME is not active.
        assert!(h.ime_state().is_none());

        // Focus via accessibility tree lookup + click.
        h.click_label("Note");
        let ime = h.ime_state().expect("IME enabled when TextInput focused");
        assert!(ime.enabled);
        assert!(ime.cursor_rect.is_some());

        // Type ASCII + CJK IME preedit + commit.
        h.type_text("Hi ");
        assert_eq!(note.get_untracked(), "Hi ");

        h.ime_preedit("にほんご", Some((0, 12)));
        // Preedit is not committed to the signal yet.
        assert_eq!(note.get_untracked(), "Hi ");

        h.ime_commit("日本語");
        assert_eq!(note.get_untracked(), "Hi 日本語");

        // Accessibility value reflects the live text.
        let field = h.find(Role::TextInput, "Note").expect("TextInput node");
        assert_eq!(field.value.as_deref(), Some("Hi 日本語"));

        // Select all, copy, backspace, paste.
        h.press_key_with(Key::Character("a".into()), cmd());
        h.press_key_with(Key::Character("c".into()), cmd());
        assert_eq!(h.clipboard_text(), "Hi 日本語");

        h.press_key(Key::Backspace);
        assert_eq!(note.get_untracked(), "");

        h.press_key_with(Key::Character("v".into()), cmd());
        assert_eq!(note.get_untracked(), "Hi 日本語");

        // Enter triggers on_submit.
        h.press_key(Key::Enter);
        assert_eq!(submitted.get_untracked(), "Hi 日本語");

        // VoiceOver SetValue updates the input directly.
        h.a11y_action(A11yAction::SetValue(field.id, "你好世界".into()));
        assert_eq!(note.get_untracked(), "你好世界");
    }

    #[test]
    fn system_appearance_animates_theme_and_accent() {
        let mut h = Harness::new(|| column((text("Hello"), button("Tap"))));
        h.set_appearance(Appearance::new(ColorScheme::Light, None));
        h.settle();
        let light_bg = theme().background;

        let custom_accent = Color::from_hex(0xFF2D55);
        h.set_appearance(Appearance::new(ColorScheme::Dark, Some(custom_accent)));
        assert_eq!(theme().accent, custom_accent);
        assert_ne!(theme().background, light_bg);
        assert!(h.wants_frame(), "theme change should animate");
        let frames = h.settle();
        assert!(frames > 3, "colour transition should spring across frames");
    }

    #[test]
    fn draggable_titlebar_and_menu_actions() {
        let menu_hits = signal(0);
        let ui = Ui::new(|| {
            column((row((text("Header"), button("Action")))
                .height(44.0)
                .width(400.0)
                .draggable(),))
        })
        .on_menu_action(move |id| {
            if id == "counter.inc" {
                menu_hits.update(|n| *n += 1);
            }
        });
        let mut h = Harness::from_ui(ui, Size::new(600.0, 400.0));

        // Clicking empty draggable header area emits DragWindow.
        let header_label = h.by_label("Header").expect("Header label");
        h.click_point(header_label.bounds.center());
        assert_eq!(h.take_commands(), vec![WindowCommand::DragWindow]);

        // Clicking a button inside the draggable header does NOT drag the window.
        h.click_label("Action");
        assert!(h.take_commands().is_empty());

        // Native menu action routes to the Ui handler.
        h.trigger_menu("counter.inc");
        assert_eq!(menu_hits.get_untracked(), 1);
    }
}
