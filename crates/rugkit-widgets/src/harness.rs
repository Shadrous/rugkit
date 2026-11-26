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

use rugkit_render::{Canvas, Color, OffscreenRenderer, Point, Size, Vec2};
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

    /// Simulates scrolling by `delta` logical pixels at `position`.
    pub fn scroll(&mut self, position: Point, delta: Vec2) {
        self.send(InputEvent::Scroll { delta, position });
    }

    /// Toggles the in-app inspector overlay.
    pub fn toggle_inspector(&mut self) {
        self.ui.toggle_inspector();
        self.step(1.0 / 120.0);
    }

    /// Whether the in-app inspector overlay is currently open.
    pub fn inspector_open(&self) -> bool {
        self.ui.inspector_open()
    }

    /// Triggers a hot-reload view rebuild (when the `Ui` was created with `Ui::new_reloadable`),
    /// preserving all signal state.
    pub fn reload_view(&mut self) -> bool {
        let ok = self.ui.reload_view();
        self.step(1.0 / 120.0);
        ok
    }

    /// Renders the latest frame offscreen via `renderer` and returns `(width, height, rgba8)`.
    pub fn render_offscreen(
        &mut self,
        renderer: &mut OffscreenRenderer,
        background: Color,
    ) -> Result<(u32, u32, Vec<u8>), String> {
        self.step(1.0 / 120.0);
        let w = (self.size.width * self.scale).round().max(1.0) as u32;
        let h = (self.size.height * self.scale).round().max(1.0) as u32;
        let rgba = renderer.render_to_rgba(&self.canvas, w, h, background)?;
        Ok((w, h, rgba))
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

    #[test]
    fn m3_controls_checkbox_toggle_radio_slider_select_tabs_and_modal() {
        use crate::{
            IconName, checkbox, direction_scope, grid, icon, image, modal, popover, radio, scroll,
            select, slider, tabs, theme_scope, toggle, tooltip, virtual_list, zstack,
        };
        use rugkit_layout::Direction;
        use rugkit_render::{ImageData, Rect, TextStyle};

        let checked = signal(false);
        let switched = signal(false);
        let choice = signal("A");
        let level = signal(50.0);
        let selected = signal(0usize);
        let active_tab = signal(0usize);
        let modal_open = signal(false);
        let pop_open = signal(false);

        let mut h = Harness::with_size(Size::new(900.0, 760.0), move || {
            zstack((
                scroll(
                    column((
                        row((
                            checkbox(checked).label("Agree"),
                            toggle(switched).label("Notifications"),
                            radio(choice, "A").label("Alpha"),
                            radio(choice, "B").label("Beta"),
                        ))
                        .gap(12.0),
                        slider(level, 0.0, 100.0).step(10.0).a11y_label("Volume"),
                        select(selected, vec!["One".into(), "Two".into()]),
                        tabs(
                            active_tab,
                            vec![
                                ("First".into(), text("Panel 1").into_any()),
                                ("Second".into(), text("Panel 2").into_any()),
                            ],
                        ),
                        popover(
                            pop_open,
                            button("Open Popover").on_click(move || pop_open.set(true)),
                            text("Popover body"),
                        ),
                        tooltip("Helpful tip", icon(IconName::Star)),
                        grid(2, (text("G1"), text("G2"))),
                        image(ImageData::solid(24, 24, Color::from_hex(0x7B61FF))),
                        theme_scope(
                            |t| t.with_accent(Color::from_hex(0xFF5A5F)),
                            direction_scope(Direction::Rtl, row((text("R1"), text("R2"))).gap(8.0)),
                        ),
                        virtual_list(100_000usize, 28.0, |p, i, r: Rect| {
                            p.text(&format!("Row {i}"), r.origin(), &TextStyle::new(13.0));
                        })
                        .height(120.0),
                        button("Open Modal").on_click(move || modal_open.set(true)),
                    ))
                    .gap(10.0),
                )
                .height(740.0),
                modal(
                    modal_open,
                    button("Close Modal").on_click(move || modal_open.set(false)),
                ),
            ))
        });

        // Checkbox & Toggle
        let agree = h.find(Role::Checkbox, "Agree").expect("Agree checkbox");
        assert_eq!(agree.checked, Some(false));
        h.a11y_click(agree.id);
        assert!(checked.get_untracked());
        assert_eq!(h.find(Role::Checkbox, "Agree").unwrap().checked, Some(true));

        let sw = h
            .find(Role::Switch, "Notifications")
            .expect("Toggle switch");
        h.a11y_click(sw.id);
        assert!(switched.get_untracked());

        // Radio
        let beta = h.find(Role::RadioButton, "Beta").expect("Beta radio");
        h.a11y_click(beta.id);
        assert_eq!(choice.get_untracked(), "B");

        // Slider via keyboard arrow
        let vol = h.find(Role::Slider, "Volume").expect("Volume slider");
        assert_eq!(vol.numeric_value, Some(50.0));
        h.a11y_action(A11yAction::Focus(vol.id));
        h.press_key(Key::ArrowRight);
        assert_eq!(level.get_untracked(), 60.0);

        // Tabs
        let second_tab = h.find(Role::Tab, "Second").expect("Second tab");
        h.a11y_click(second_tab.id);
        assert_eq!(active_tab.get_untracked(), 1);
        assert!(h.find(Role::Label, "Panel 2").is_some());

        // Modal open & close
        assert!(h.by_role(Role::Dialog).is_empty());
        h.click_label("Open Modal");
        assert_eq!(h.by_role(Role::Dialog).len(), 1);
        h.click_label("Close Modal");
        assert!(h.by_role(Role::Dialog).is_empty());

        // Inspector toggle
        assert!(!h.inspector_open());
        h.toggle_inspector();
        assert!(h.inspector_open());
    }

    #[test]
    fn offscreen_snapshot_renders_without_failing() {
        use crate::{SnapshotOutcome, check_snapshot};

        let mut h = Harness::with_size(Size::new(320.0, 200.0), || {
            column((text("Snapshot").size(24.0), button("Primary"))).gap(12.0)
        });
        let outcome = check_snapshot("smoke", &mut h, theme().background, 4, None);
        match outcome {
            SnapshotOutcome::CreatedBaseline { pixels } => assert!(pixels > 0),
            SnapshotOutcome::Compared(diff) => assert_eq!(diff.differing_pixels, 0),
            SnapshotOutcome::Skipped(_) => {}
        }
    }

    #[test]
    fn reactive_canvas_tracks_signals_and_hit_tests() {
        use crate::{HitRegions, canvas};
        use rugkit_render::Rect;

        let value = signal(10.0f64);
        let draws = Rc::new(RefCell::new(0usize));
        let draws_in_cb = draws.clone();
        let hits = Rc::new(RefCell::new(HitRegions::<&'static str>::new()));
        let hits_draw = hits.clone();
        let hits_click = hits;
        let clicked = signal(None::<&'static str>);

        let mut h = Harness::with_size(Size::new(300.0, 200.0), move || {
            canvas(move |p, rect| {
                *draws_in_cb.borrow_mut() += 1;
                let bar_h = value.get();
                let local_bar = Rect::from_xywh(20.0, 20.0, 80.0, bar_h);
                let mut reg = hits_draw.borrow_mut();
                reg.clear();
                reg.push_rect("bar-a", local_bar);
                p.fill_rect(
                    Rect::from_xywh(
                        rect.x0 + local_bar.x0,
                        rect.y0 + local_bar.y0,
                        local_bar.width(),
                        local_bar.height(),
                    ),
                    6.0,
                    Color::from_hex(0x7B61FF),
                );
            })
            .width(200.0)
            .height(120.0)
            .on_click(move |pt| {
                let hit = hits_click.borrow().hit_test(pt).copied();
                clicked.set(hit);
            })
        });

        assert!(!h.wants_frame(), "canvas should idle after initial draw");
        let before = *draws.borrow();

        // Changing a signal read inside `canvas` automatically schedules a redraw!
        value.set(60.0);
        assert!(
            h.wants_frame(),
            "signal change read by canvas must schedule a redraw"
        );
        h.step(1.0 / 120.0);
        assert!(*draws.borrow() > before);

        // Clicking inside the hit region invokes on_click with canvas-local coords.
        let canvas_node = h.by_role(Role::Image).into_iter().next().unwrap();
        let click_pt = Point::new(canvas_node.bounds.x0 + 40.0, canvas_node.bounds.y0 + 40.0);
        h.click_point(click_pt);
        assert_eq!(clicked.get_untracked(), Some("bar-a"));
    }

    #[test]
    fn table_supports_100k_rows_sort_resize_and_selection() {
        use crate::{Column, SelectionMode, SortDirection, table};
        use std::collections::BTreeSet;

        #[derive(Clone)]
        struct Item {
            id: usize,
            name: String,
        }

        let data: Vec<Item> = (0..100_000)
            .map(|i| Item {
                id: i,
                name: format!("Row {i:06}"),
            })
            .collect();
        let rows = signal(data);
        let selected = signal(BTreeSet::new());
        let sort = signal(None::<(usize, SortDirection)>);

        let mut h = Harness::with_size(Size::new(600.0, 400.0), move || {
            table(
                rows,
                vec![
                    Column::text("id", "ID", 100.0, |r: &Item| r.id.to_string())
                        .sort_by(|a, b| a.id.cmp(&b.id)),
                    Column::text("name", "Name", 240.0, |r: &Item| r.name.clone()),
                ],
            )
            .row_height(32.0)
            .header_height(36.0)
            .selection_mode(SelectionMode::Multiple)
            .selected(selected)
            .sort(sort)
            .width(500.0)
            .height(300.0)
        });

        let tbl = h.find(Role::List, "Table").expect("Table a11y node");
        let b = tbl.bounds;

        // Click first body row -> selects row 0.
        h.click_point(Point::new(b.x0 + 50.0, b.y0 + 36.0 + 16.0));
        assert_eq!(selected.get_untracked(), BTreeSet::from([0]));

        // ArrowDown moves selection to row 1.
        h.press_key(Key::ArrowDown);
        assert_eq!(selected.get_untracked(), BTreeSet::from([1]));

        // Clicking header column 0 cycles sort: None -> Asc -> Desc.
        h.click_point(Point::new(b.x0 + 30.0, b.y0 + 18.0));
        assert_eq!(sort.get_untracked(), Some((0, SortDirection::Ascending)));
        h.click_point(Point::new(b.x0 + 30.0, b.y0 + 18.0));
        assert_eq!(sort.get_untracked(), Some((0, SortDirection::Descending)));

        // After descending sort on 100k rows, clicking first visible row selects index 99_999.
        h.click_point(Point::new(b.x0 + 50.0, b.y0 + 36.0 + 16.0));
        assert_eq!(selected.get_untracked(), BTreeSet::from([99_999]));

        // Scroll 100k table down without panicking or stalling.
        h.scroll(b.center(), Vec2::new(0.0, -5000.0));
        assert!(h.settle() < 200);
    }

    #[test]
    fn hot_reload_rebuilds_view_while_keeping_signal_state() {
        let generation = Rc::new(RefCell::new(1usize));
        let gen_in_view = generation.clone();
        let ui = Ui::new_reloadable(
            || signal(0i32),
            move |&count| {
                let g = *gen_in_view.borrow();
                column((
                    text(move || format!("v{g}: {}", count.get())),
                    button("+").on_click(move || count.update(|c| *c += 1)),
                ))
            },
        );
        let mut h = Harness::from_ui(ui, Size::new(400.0, 300.0));
        h.click_label("+");
        h.click_label("+");
        assert!(h.find(Role::Label, "v1: 2").is_some());

        // Simulate a hot-reload patch (v2 view): count signal MUST stay at 2!
        *generation.borrow_mut() = 2;
        assert!(h.reload_view());
        assert!(h.find(Role::Label, "v2: 2").is_some());

        // Clicking after reload still updates the preserved signal.
        h.click_label("+");
        assert!(h.find(Role::Label, "v2: 3").is_some());
    }

    #[test]
    fn async_resource_delivers_result_to_ui_thread() {
        use crate::{ResourceState, resource};

        let res = resource(async { Ok::<_, String>(42usize) });
        let mut h = Harness::new(move || {
            text(move || match res.get() {
                ResourceState::Loading => "loading".into(),
                ResourceState::Ready(v) => format!("ready:{v}"),
                ResourceState::Error(e) => format!("err:{e}"),
            })
        });
        for _ in 0..50 {
            if h.find(Role::Label, "ready:42").is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
            h.step(1.0 / 120.0);
        }
        assert!(h.find(Role::Label, "ready:42").is_some());
    }
}
