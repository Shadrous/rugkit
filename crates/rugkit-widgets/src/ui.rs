//! [`Ui`]: hosts a view tree in a window, routes input, focus, IME, accessibility and animations.

use std::cell::RefCell;
use std::rc::Rc;

use rugkit_layout::{Align, Justify, LayoutStyle, Length};
use rugkit_reactive::{Owner, Signal, batch};
use rugkit_render::{Color, Painter, Point, Rect, Size};
use rugkit_shell::{
    A11yAction, A11yTree, App, FrameInfo, ImeEvent, ImeState, InputEvent, Key, Modifiers,
    PointerButton, WakeHandle, WindowCommand, WindowSpec,
};

use crate::controls::paint_tooltip;
use crate::inspector::Inspector;
use crate::motion::{Animated, Motion};
use crate::theme::{set_appearance, theme};
use crate::tree::{Content, NodeId, PointerKind, TreeHandle};
use crate::view::{BuildCx, View};

/// Clipboard storage used by [`Ui`]: system clipboard in real windows, or in-memory in tests.
#[derive(Clone)]
pub(crate) enum Clipboard {
    System,
    Memory(Rc<RefCell<String>>),
}

impl Clipboard {
    fn get_text(&self) -> Option<String> {
        match self {
            Self::System => rugkit_shell::clipboard::get_text(),
            Self::Memory(buf) => {
                let s = buf.borrow();
                (!s.is_empty()).then(|| s.clone())
            }
        }
    }

    fn set_text(&self, text: &str) {
        match self {
            Self::System => {
                let _ = rugkit_shell::clipboard::set_text(text);
            }
            Self::Memory(buf) => {
                *buf.borrow_mut() = text.to_owned();
            }
        }
    }
}

type MenuHandler = Rc<dyn Fn(&str)>;

/// A view tree hosted in a window. Implements [`App`], so it can be passed to
/// `rugkit_shell::run` (the `rugkit` facade's `launch` does this for you).
///
/// Frames are drawn only while something changes or animates. An idle window costs nothing.
pub struct Ui {
    pub(crate) tree: TreeHandle,
    owner: Owner,
    hovered: Option<NodeId>,
    captured: Option<NodeId>,
    focused: Option<NodeId>,
    pointer_pos: Option<Point>,
    modifiers: Modifiers,
    animating: bool,
    last_size: Option<Size>,
    background: Animated<Color>,
    bg_initialized: bool,
    commands: Vec<WindowCommand>,
    new_windows: Vec<WindowSpec>,
    clipboard: Clipboard,
    on_menu_action: Option<MenuHandler>,
    inspector: Inspector,
    inspector_allowed: bool,
}

impl std::fmt::Debug for Ui {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ui")
            .field("animating", &self.animating)
            .finish_non_exhaustive()
    }
}

impl Ui {
    /// Builds the view returned by `view` (centred in the window) under a fresh reactive owner.
    pub fn new<V: View>(view: impl FnOnce() -> V) -> Self {
        let tree = TreeHandle::new();
        let owner = Owner::new();
        let root = owner.run(|| {
            let mut cx = BuildCx::new(tree.clone());
            let content = view().build(&mut cx);
            let root = cx.node(
                LayoutStyle {
                    width: Length::Fraction(1.0),
                    height: Length::Fraction(1.0),
                    align: Align::Center,
                    justify: Justify::Center,
                    ..Default::default()
                },
                Content::None,
            );
            cx.set_children(root, vec![content]);
            root
        });
        tree.with(|t| t.root = Some(root));
        Self {
            tree,
            owner,
            hovered: None,
            captured: None,
            focused: None,
            pointer_pos: None,
            modifiers: Modifiers::default(),
            animating: false,
            last_size: None,
            background: Animated::new(theme().background),
            bg_initialized: false,
            commands: Vec::new(),
            new_windows: Vec::new(),
            clipboard: Clipboard::System,
            on_menu_action: None,
            inspector: Inspector::default(),
            inspector_allowed: cfg!(debug_assertions),
        }
    }

    /// Enables or disables the `⌘⌥I` in-app inspector (enabled by default in debug builds).
    pub fn inspector_enabled(mut self, enabled: bool) -> Self {
        self.inspector_allowed = enabled;
        self
    }

    /// Toggles the in-app inspector overlay.
    pub fn toggle_inspector(&mut self) {
        self.inspector.open = !self.inspector.open;
        self.tree.with(|t| t.mark_needs_frame());
    }

    /// Whether the in-app inspector overlay is currently open.
    pub fn inspector_open(&self) -> bool {
        self.inspector.open
    }

    /// Queues a new window to be opened on the shared UI thread.
    pub fn open_window(&mut self, spec: WindowSpec) {
        self.new_windows.push(spec);
        self.tree.with(|t| t.mark_needs_frame());
    }

    /// Registers a handler invoked when a native menu bar item is activated (by its item ID).
    pub fn on_menu_action(mut self, f: impl Fn(&str) + 'static) -> Self {
        self.on_menu_action = Some(Rc::new(f));
        self
    }

    pub(crate) fn with_memory_clipboard(mut self, buf: Rc<RefCell<String>>) -> Self {
        self.clipboard = Clipboard::Memory(buf);
        self
    }

    fn signals(&self, id: Option<NodeId>) -> Option<(Signal<bool>, Signal<bool>, Signal<bool>)> {
        let id = id?;
        self.tree.with(|t| {
            t.nodes
                .get(id)
                .and_then(|n| n.interaction.as_ref())
                .map(|i| (i.hovered, i.pressed, i.focused))
        })
    }

    fn dispatch_pointer(&self, id: NodeId, kind: PointerKind, pos: Point) {
        let mods = self.modifiers;
        let hit = self.tree.with(|t| {
            let n = t.nodes.get(id)?;
            let rect = n.visual.frame.target();
            n.interaction
                .as_ref()
                .and_then(|i| i.on_pointer.clone())
                .map(|h| (h, rect))
        });
        if let Some((handler, rect)) = hit {
            batch(|| handler(kind, pos, rect, mods));
        }
    }

    fn set_hover(&mut self, target: Option<NodeId>) {
        if target == self.hovered {
            return;
        }
        let prev = self.hovered;
        let old = self.signals(self.hovered);
        let new = self.signals(target);
        self.hovered = target;
        if let Some(prev_id) = prev {
            self.dispatch_pointer(
                prev_id,
                PointerKind::Leave,
                self.pointer_pos.unwrap_or(Point::ZERO),
            );
        }
        batch(|| {
            if let Some((h, _, _)) = old {
                h.set_if_changed(false);
            }
            if let Some((h, _, _)) = new {
                h.set_if_changed(true);
            }
        });
    }

    fn set_focus(&mut self, target: Option<NodeId>) {
        let target = target.filter(|&id| {
            self.tree.with(|t| {
                t.nodes
                    .get(id)
                    .and_then(|n| n.interaction.as_ref())
                    .is_some_and(|i| i.focusable)
            })
        });
        if target == self.focused {
            return;
        }
        // Clear any uncommitted IME preedit when leaving a text input.
        if let Some(old_id) = self.focused {
            self.tree.with(|t| {
                if let Some(n) = t.nodes.get_mut(old_id)
                    && let Content::TextInput(state) = &mut n.content
                {
                    state.preedit = None;
                }
            });
        }
        let old = self.signals(self.focused);
        let new = self.signals(target);
        self.focused = target;
        batch(|| {
            if let Some((_, _, f)) = old {
                f.set_if_changed(false);
            }
            if let Some((_, _, f)) = new {
                f.set_if_changed(true);
            }
        });
        self.tree.with(|t| t.mark_needs_frame());
    }

    fn move_focus(&mut self, backwards: bool) {
        let list = self.tree.with(|t| t.focusable_nodes());
        if list.is_empty() {
            return;
        }
        let next = match self
            .focused
            .and_then(|f| list.iter().position(|&id| id == f))
        {
            None => {
                if backwards {
                    list[list.len() - 1]
                } else {
                    list[0]
                }
            }
            Some(idx) => {
                if backwards {
                    list[(idx + list.len() - 1) % list.len()]
                } else {
                    list[(idx + 1) % list.len()]
                }
            }
        };
        self.set_focus(Some(next));
    }

    fn activate_node(&mut self, id: NodeId) {
        let handler = self.tree.with(|t| {
            t.nodes
                .get(id)
                .and_then(|n| n.interaction.as_ref())
                .and_then(|i| i.on_click.clone())
        });
        if let Some(handler) = handler {
            batch(|| handler());
        }
    }

    fn handle_key_down(&mut self, key: &Key, modifiers: Modifiers, text: Option<&str>) {
        if self.inspector_allowed && self.inspector.handle_key(key, modifiers) {
            self.tree.with(|t| t.mark_needs_frame());
            return;
        }

        // Tab / Shift+Tab always cycles focus.
        if *key == Key::Tab && !modifiers.ctrl && !modifiers.alt && !modifiers.meta {
            self.move_focus(modifiers.shift);
            return;
        }

        let Some(focused) = self.focused else { return };

        let (is_text_input, on_key) = self.tree.with(|t| {
            let n = t.nodes.get(focused);
            let ti = n.is_some_and(|n| matches!(n.content, Content::TextInput(_)));
            let ok = n
                .and_then(|n| n.interaction.as_ref())
                .and_then(|i| i.on_key.clone());
            (ti, ok)
        });

        if let Some(handler) = on_key
            && batch(|| handler(key, modifiers))
        {
            return;
        }

        if is_text_input {
            self.handle_text_input_key(focused, key, modifiers, text);
            return;
        }

        // Focused button/checkbox/switch/radio/tab: Enter or Space activates it.
        if matches!(key, Key::Enter | Key::Space) && modifiers.is_empty() {
            self.activate_node(focused);
        }
    }

    fn handle_text_input_key(
        &mut self,
        id: NodeId,
        key: &Key,
        modifiers: Modifiers,
        text: Option<&str>,
    ) {
        // Handle clipboard & select-all shortcuts first (outside tree borrow when writing signals).
        if modifiers.command() && !modifiers.alt {
            if let Key::Character(c) = key {
                match c.to_ascii_lowercase().as_str() {
                    "a" => {
                        self.tree.with(|t| {
                            if let Some(n) = t.nodes.get_mut(id)
                                && let Content::TextInput(s) = &mut n.content
                            {
                                s.select_all();
                                t.mark_needs_frame();
                            }
                        });
                        return;
                    }
                    "c" => {
                        let sel = self.tree.with(|t| {
                            t.nodes.get(id).and_then(|n| match &n.content {
                                Content::TextInput(s) => Some(s.selected_text()),
                                _ => None,
                            })
                        });
                        if let Some(sel) = sel
                            && !sel.is_empty()
                        {
                            self.clipboard.set_text(&sel);
                        }
                        return;
                    }
                    "x" => {
                        let sel = self.tree.with(|t| {
                            t.nodes.get(id).and_then(|n| match &n.content {
                                Content::TextInput(s) => Some(s.selected_text()),
                                _ => None,
                            })
                        });
                        if let Some(sel) = sel
                            && !sel.is_empty()
                        {
                            self.clipboard.set_text(&sel);
                            self.mutate_text_input(id, |s| s.backspace());
                        }
                        return;
                    }
                    "v" => {
                        if let Some(paste) = self.clipboard.get_text() {
                            // Strip newlines for single-line input.
                            let clean = paste.replace(['\r', '\n'], " ");
                            self.mutate_text_input(id, |s| s.insert_text(&clean));
                        }
                        return;
                    }
                    _ => {}
                }
            }
            if *key == Key::ArrowLeft {
                self.tree.with(|t| {
                    if let Some(n) = t.nodes.get_mut(id)
                        && let Content::TextInput(s) = &mut n.content
                    {
                        s.move_home(modifiers.shift);
                        t.mark_needs_frame();
                    }
                });
                return;
            }
            if *key == Key::ArrowRight {
                self.tree.with(|t| {
                    if let Some(n) = t.nodes.get_mut(id)
                        && let Content::TextInput(s) = &mut n.content
                    {
                        s.move_end(modifiers.shift);
                        t.mark_needs_frame();
                    }
                });
                return;
            }
        }

        match key {
            Key::Backspace => self.mutate_text_input(id, |s| s.backspace()),
            Key::Delete => self.mutate_text_input(id, |s| s.delete_forward()),
            Key::ArrowLeft => {
                self.tree.with(|t| {
                    if let Some(n) = t.nodes.get_mut(id)
                        && let Content::TextInput(s) = &mut n.content
                    {
                        s.move_left(modifiers.shift);
                        t.mark_needs_frame();
                    }
                });
            }
            Key::ArrowRight => {
                self.tree.with(|t| {
                    if let Some(n) = t.nodes.get_mut(id)
                        && let Content::TextInput(s) = &mut n.content
                    {
                        s.move_right(modifiers.shift);
                        t.mark_needs_frame();
                    }
                });
            }
            Key::Home => {
                self.tree.with(|t| {
                    if let Some(n) = t.nodes.get_mut(id)
                        && let Content::TextInput(s) = &mut n.content
                    {
                        s.move_home(modifiers.shift);
                        t.mark_needs_frame();
                    }
                });
            }
            Key::End => {
                self.tree.with(|t| {
                    if let Some(n) = t.nodes.get_mut(id)
                        && let Content::TextInput(s) = &mut n.content
                    {
                        s.move_end(modifiers.shift);
                        t.mark_needs_frame();
                    }
                });
            }
            Key::Enter => {
                let submit = self.tree.with(|t| {
                    t.nodes.get(id).and_then(|n| match &n.content {
                        Content::TextInput(s) => {
                            s.on_submit.clone().map(|f| (f, s.value.get_untracked()))
                        }
                        _ => None,
                    })
                });
                if let Some((f, val)) = submit {
                    batch(|| f(val));
                }
            }
            Key::Escape => self.set_focus(None),
            Key::Space if !modifiers.ctrl && !modifiers.meta => {
                self.mutate_text_input(id, |s| s.insert_text(" "));
            }
            _ => {
                if !modifiers.ctrl && !modifiers.meta && !modifiers.alt {
                    let insert = text.or(match key {
                        Key::Character(s) => Some(s.as_str()),
                        _ => None,
                    });
                    if let Some(s) = insert
                        && !s.is_empty()
                        && !s.chars().all(char::is_control)
                    {
                        self.mutate_text_input(id, |st| st.insert_text(s));
                    }
                }
            }
        }
    }

    /// Runs a mutation on a `TextInputState` while avoiding signal writes during a tree borrow.
    fn mutate_text_input(&mut self, id: NodeId, f: impl FnOnce(&mut crate::tree::TextInputState)) {
        // Temporarily extract the TextInputState? No: `insert_text` writes `value` signal,
        // which triggers effects that borrow `TreeHandle`. So we swap the state out, run `f`
        // inside `batch`, and put it back!
        let extracted = self.tree.with(|t| {
            let n = t.nodes.get_mut(id)?;
            match std::mem::replace(&mut n.content, Content::None) {
                Content::TextInput(state) => Some(state),
                other => {
                    n.content = other;
                    None
                }
            }
        });
        if let Some(mut state) = extracted {
            batch(|| f(&mut state));
            self.tree.with(|t| {
                if let Some(n) = t.nodes.get_mut(id) {
                    n.content = Content::TextInput(state);
                    t.mark_needs_frame();
                }
            });
        }
    }

    fn handle_ime(&mut self, ime: &ImeEvent) {
        let Some(focused) = self.focused else { return };
        match ime {
            ImeEvent::Enabled => {}
            ImeEvent::Preedit(text, range) => {
                self.tree.with(|t| {
                    if let Some(n) = t.nodes.get_mut(focused)
                        && let Content::TextInput(s) = &mut n.content
                    {
                        s.preedit = if text.is_empty() {
                            None
                        } else {
                            Some((text.clone(), *range))
                        };
                        t.mark_needs_frame();
                    }
                });
            }
            ImeEvent::Commit(text) => {
                self.mutate_text_input(focused, |s| {
                    s.preedit = None;
                    if !text.is_empty() {
                        s.insert_text(text);
                    }
                });
            }
            ImeEvent::Disabled => {
                self.tree.with(|t| {
                    if let Some(n) = t.nodes.get_mut(focused)
                        && let Content::TextInput(s) = &mut n.content
                    {
                        s.preedit = None;
                        t.mark_needs_frame();
                    }
                });
            }
        }
    }
}

impl App for Ui {
    fn attach(&mut self, wake: WakeHandle) {
        self.tree.with(|t| t.waker = Some(wake));
    }

    fn event(&mut self, event: &InputEvent) {
        match event {
            InputEvent::PointerMove(p) => {
                self.pointer_pos = Some(*p);
                let target = self.tree.with(|t| t.hit_test(*p));
                self.set_hover(target);
                if let Some(captured) = self.captured {
                    self.dispatch_pointer(captured, PointerKind::Move, *p);
                    let drag = self.tree.with(|t| {
                        let n = t.nodes.get(captured)?;
                        let f = n.visual.frame.target();
                        n.interaction
                            .as_ref()
                            .and_then(|i| i.on_drag.clone())
                            .map(|h| (h, f))
                    });
                    if let Some((handler, rect)) = drag {
                        batch(|| handler(*p, rect));
                    } else if let Some((_, pressed, _)) = self.signals(Some(captured)) {
                        pressed.set_if_changed(target == Some(captured));
                    }
                } else if let Some(id) = target {
                    self.dispatch_pointer(id, PointerKind::Move, *p);
                }
            }
            InputEvent::PointerDown(p, btn) => {
                self.pointer_pos = Some(*p);
                let (target, draggable) =
                    self.tree.with(|t| (t.hit_test(*p), t.is_draggable_at(*p)));
                self.set_hover(target);
                if *btn == PointerButton::Primary {
                    self.set_focus(target);
                    self.captured = target;
                    if let Some((_, pressed, _)) = self.signals(target) {
                        pressed.set(true);
                    } else if draggable {
                        self.commands.push(WindowCommand::DragWindow);
                    }
                }
                if let Some(id) = target {
                    self.dispatch_pointer(id, PointerKind::Down(*btn), *p);
                    if *btn == PointerButton::Primary {
                        let drag = self.tree.with(|t| {
                            let n = t.nodes.get(id)?;
                            let f = n.visual.frame.target();
                            n.interaction
                                .as_ref()
                                .and_then(|i| i.on_drag.clone())
                                .map(|h| (h, f))
                        });
                        if let Some((handler, rect)) = drag {
                            batch(|| handler(*p, rect));
                        }
                    }
                }
            }
            InputEvent::PointerUp(p, btn) => {
                self.pointer_pos = Some(*p);
                let target = self.tree.with(|t| t.hit_test(*p));
                if *btn == PointerButton::Primary {
                    let Some(captured) = self.captured.take() else {
                        return;
                    };
                    self.dispatch_pointer(captured, PointerKind::Up(*btn), *p);
                    if let Some((_, pressed, _)) = self.signals(Some(captured)) {
                        pressed.set_if_changed(false);
                    }
                    if target == Some(captured) {
                        self.activate_node(captured);
                    }
                } else if let Some(id) = target {
                    self.dispatch_pointer(id, PointerKind::Up(*btn), *p);
                }
            }
            InputEvent::PointerLeave => {
                self.pointer_pos = None;
                self.set_hover(None);
            }
            InputEvent::Scroll { delta, position } => {
                let target = self.tree.with(|t| t.scroll_target_at(*position));
                if let Some(id) = target {
                    let custom_scroll = self.tree.with(|t| {
                        t.nodes
                            .get(id)
                            .and_then(|n| n.interaction.as_ref())
                            .and_then(|i| i.on_scroll.clone())
                    });
                    if let Some(handler) = custom_scroll {
                        batch(|| handler(*delta));
                    } else {
                        self.tree.with(|t| {
                            if let Some(n) = t.nodes.get_mut(id) {
                                let next =
                                    (n.visual.scroll_y.target() - delta.y).clamp(0.0, n.scroll_max);
                                n.visual.scroll_y.set_target(next);
                                t.mark_needs_frame();
                            }
                        });
                    }
                }
            }
            InputEvent::KeyDown {
                key,
                modifiers,
                text,
                ..
            } => {
                self.modifiers = *modifiers;
                self.handle_key_down(key, *modifiers, text.as_deref());
            }
            InputEvent::KeyUp { modifiers, .. } => {
                self.modifiers = *modifiers;
            }
            InputEvent::Ime(ime) => self.handle_ime(ime),
            InputEvent::Focused(false) => {
                // Keep logical focus for when the window returns, but clear uncommitted preedit.
                if let Some(focused) = self.focused {
                    self.tree.with(|t| {
                        if let Some(n) = t.nodes.get_mut(focused)
                            && let Content::TextInput(s) = &mut n.content
                        {
                            s.preedit = None;
                        }
                    });
                }
            }
            InputEvent::AppearanceChanged(appearance) => {
                batch(|| set_appearance(*appearance));
                let bg = theme().background;
                if self.bg_initialized {
                    self.background.set_target(bg);
                } else {
                    self.background.snap(bg);
                }
                self.tree.with(|t| t.mark_needs_frame());
            }
            InputEvent::MenuAction(id) => {
                if let Some(handler) = self.on_menu_action.clone() {
                    batch(|| handler(id));
                }
            }
            _ => {}
        }
    }

    fn draw(&mut self, painter: &mut Painter<'_>, frame: &FrameInfo) {
        // Clamp so a hitch can't fling springs across the screen; apply inspector slow-mo.
        let dt = frame.dt.as_secs_f64().min(1.0 / 20.0) * self.inspector.time_scale;
        let resized = self.last_size != Some(frame.size);
        self.last_size = Some(frame.size);

        let target_bg = theme().background;
        if !self.bg_initialized {
            self.background.snap(target_bg);
            self.bg_initialized = true;
        } else {
            self.background.set_target(target_bg);
        }
        let bg_moving = self.background.step(
            dt,
            Motion::Spring(theme().color_spring),
            frame.reduce_motion,
        );

        painter.fill_rect(
            Rect::from_origin_size(Point::ZERO, frame.size),
            0.0,
            self.background.value(),
        );
        let focused = self.focused;
        let pointer_pos = self.pointer_pos;
        let tree = self.tree.clone();
        tree.with(|t| {
            if t.needs_layout || resized {
                t.compute_layout(frame.size, painter);
            }
            self.animating = t.step(dt, frame.reduce_motion) || bg_moving;
            t.paint(painter, focused);
            if let Some(pos) = pointer_pos
                && let Some((tip, anchor)) = t.tooltip_at(pos)
            {
                paint_tooltip(painter, &tip, anchor);
            }
            self.inspector.paint(painter, t, focused, frame);
            t.needs_frame = false;
        });
    }

    fn wants_frame(&self) -> bool {
        self.animating || self.tree.with(|t| t.needs_frame)
    }

    fn ime_state(&self) -> Option<ImeState> {
        let focused = self.focused?;
        self.tree.with(|t| {
            let n = t.nodes.get(focused)?;
            match &n.content {
                Content::TextInput(s) => Some(ImeState {
                    enabled: true,
                    cursor_rect: Some(s.cursor_rect),
                }),
                _ => None,
            }
        })
    }

    fn a11y_tree(&mut self) -> Option<A11yTree> {
        let size = self.last_size.unwrap_or_default();
        Some(self.tree.with(|t| t.a11y_tree(self.focused, size)))
    }

    fn a11y_action(&mut self, action: &A11yAction) {
        match action {
            A11yAction::Click(id) => {
                let node_id = NodeId::from_a11y(*id);
                if self.tree.with(|t| t.nodes.contains_key(node_id)) {
                    self.set_focus(Some(node_id));
                    self.activate_node(node_id);
                }
            }
            A11yAction::Focus(id) => {
                let node_id = NodeId::from_a11y(*id);
                self.set_focus(Some(node_id));
            }
            A11yAction::Blur(id) => {
                let node_id = NodeId::from_a11y(*id);
                if self.focused == Some(node_id) {
                    self.set_focus(None);
                }
            }
            A11yAction::SetValue(id, value) => {
                let node_id = NodeId::from_a11y(*id);
                self.mutate_text_input(node_id, |s| s.set_value(value.clone()));
            }
            A11yAction::ReplaceSelectedText(id, value) => {
                let node_id = NodeId::from_a11y(*id);
                self.mutate_text_input(node_id, |s| s.insert_text(value));
            }
        }
    }

    fn take_commands(&mut self) -> Vec<WindowCommand> {
        std::mem::take(&mut self.commands)
    }

    fn take_new_windows(&mut self) -> Vec<WindowSpec> {
        std::mem::take(&mut self.new_windows)
    }
}

impl Drop for Ui {
    fn drop(&mut self) {
        self.owner.dispose();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{button, column, text};
    use rugkit_reactive::signal;
    use rugkit_render::{Canvas, Point};
    use rugkit_shell::{Appearance, FrameStats, InputEvent};
    use std::time::Duration;

    fn frame(size: Size, dt: f64, reduce_motion: bool) -> FrameInfo {
        FrameInfo {
            size,
            scale: 2.0,
            time: Duration::ZERO,
            dt: Duration::from_secs_f64(dt),
            stats: FrameStats::default(),
            reduce_motion,
            appearance: Appearance::default(),
        }
    }

    fn draw(ui: &mut Ui, canvas: &mut Canvas, dt: f64) {
        let mut p = canvas.begin_frame(2.0);
        ui.draw(&mut p, &frame(Size::new(400.0, 300.0), dt, false));
    }

    /// Draws until every animation settles; returns how many frames that took.
    fn settle(ui: &mut Ui, canvas: &mut Canvas) -> usize {
        let mut frames = 0;
        loop {
            draw(ui, canvas, 1.0 / 120.0);
            frames += 1;
            if !ui.wants_frame() {
                return frames;
            }
            assert!(frames < 2000, "animations never settled");
        }
    }

    fn button_node(ui: &Ui) -> NodeId {
        ui.tree.with(|t| {
            t.nodes
                .iter()
                .find(|(_, n)| n.interaction.is_some())
                .map(|(id, _)| id)
                .expect("a button exists")
        })
    }

    #[test]
    fn click_updates_state_text_and_animates_then_idles() {
        let count = signal(0);
        let mut ui = Ui::new(move || {
            column((
                text(move || count.get().to_string()).size(40.0),
                button("Add").on_click(move || count.update(|c| *c += 1)),
            ))
        });
        let mut canvas = Canvas::new();
        settle(&mut ui, &mut canvas);
        assert!(!ui.wants_frame(), "idle UI must not request frames");

        let id = button_node(&ui);
        let center = ui.tree.with(|t| t.nodes[id].visual.frame.target().center());

        ui.event(&InputEvent::PointerMove(center));
        assert!(ui.wants_frame(), "hover must schedule a frame");
        let rest = 6.0;
        let hover_elevation = ui.tree.with(|t| t.nodes[id].visual.elevation.target());
        assert!(hover_elevation > rest, "hover lifts the button");

        ui.event(&InputEvent::PointerDown(center, PointerButton::Primary));
        let pressed_scale = ui.tree.with(|t| t.nodes[id].visual.scale.target());
        assert!(pressed_scale < 1.0, "press squashes the button");

        ui.event(&InputEvent::PointerUp(center, PointerButton::Primary));
        assert_eq!(count.get_untracked(), 1);

        let label = ui.tree.with(|t| {
            t.nodes
                .values()
                .find_map(|n| match &n.content {
                    Content::Text { text, .. } if text.chars().all(|c| c.is_ascii_digit()) => {
                        Some(text.clone())
                    }
                    _ => None,
                })
                .expect("count label")
        });
        assert_eq!(label, "1");

        let frames = settle(&mut ui, &mut canvas);
        assert!(
            frames > 5,
            "springs should take several frames, took {frames}"
        );
        assert!(!ui.wants_frame());
    }

    #[test]
    fn release_outside_does_not_click() {
        let clicks = signal(0);
        let mut ui = Ui::new(move || button("Go").on_click(move || clicks.update(|c| *c += 1)));
        let mut canvas = Canvas::new();
        settle(&mut ui, &mut canvas);
        let id = button_node(&ui);
        let center = ui.tree.with(|t| t.nodes[id].visual.frame.target().center());
        ui.event(&InputEvent::PointerDown(center, PointerButton::Primary));
        ui.event(&InputEvent::PointerMove(Point::new(1.0, 1.0)));
        let pressed = ui.tree.with(|t| t.nodes[id].visual.scale.target());
        assert_eq!(pressed, 1.0, "dragging off releases the pressed look");
        ui.event(&InputEvent::PointerUp(
            Point::new(1.0, 1.0),
            PointerButton::Primary,
        ));
        assert_eq!(clicks.get_untracked(), 0);
    }

    #[test]
    fn layout_changes_animate_and_reduce_motion_snaps() {
        let wide = signal(false);
        let mut ui = Ui::new(move || {
            column((button("x"),)).width(move || if wide.get() { 300.0 } else { 100.0 })
        });
        let mut canvas = Canvas::new();
        settle(&mut ui, &mut canvas);
        let root_child = ui.tree.with(|t| t.nodes[t.root.unwrap()].children[0]);

        wide.set(true);
        draw(&mut ui, &mut canvas, 1.0 / 120.0);
        let mid = ui
            .tree
            .with(|t| t.nodes[root_child].visual.frame.value().width());
        assert!(mid > 100.0 && mid < 300.0, "width springs, got {mid}");
        settle(&mut ui, &mut canvas);

        wide.set(false);
        let mut p = canvas.begin_frame(2.0);
        ui.draw(&mut p, &frame(Size::new(400.0, 300.0), 1.0 / 120.0, true));
        let snapped = ui
            .tree
            .with(|t| t.nodes[root_child].visual.frame.value().width());
        assert_eq!(snapped, 100.0, "reduce motion snaps geometry");
    }
}
