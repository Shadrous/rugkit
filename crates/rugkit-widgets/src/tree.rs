//! The retained node tree: layout, animation stepping, painting, hit-testing and accessibility.

use std::cell::RefCell;
use std::rc::Rc;

use rugkit_layout::{LayoutId, LayoutStyle, LayoutTree};
use rugkit_reactive::Signal;
use rugkit_render::{Brush, Color, Painter, Point, Rect, Shadow, Size, TextStyle, Transform, Vec2};
use rugkit_shell::{A11yId, A11yNode, A11yTree, Role, WakeHandle};
use slotmap::{Key, KeyData, SlotMap};

use crate::motion::{Animated, Motion};
use crate::theme::theme;

slotmap::new_key_type! {
    /// Identifies a node in the view tree.
    pub struct NodeId;
}

impl NodeId {
    /// Converts this node ID to its stable accessibility ID.
    pub(crate) fn to_a11y(self) -> A11yId {
        A11yId(self.data().as_ffi())
    }

    /// Recovers a `NodeId` from an accessibility ID.
    pub(crate) fn from_a11y(id: A11yId) -> Self {
        Self::from(KeyData::from_ffi(id.0))
    }
}

/// Root window accessibility ID (slotmap keys are never 0).
pub(crate) const WINDOW_A11Y_ID: A11yId = A11yId(0);

/// State for an editable single-line text input node.
pub(crate) struct TextInputState {
    pub value: Signal<String>,
    pub placeholder: String,
    pub style: TextStyle,
    /// Caret position as a UTF-8 byte offset into `value`.
    pub cursor: usize,
    /// Selection anchor as a UTF-8 byte offset into `value`.
    pub anchor: usize,
    /// Active IME composition text and optional highlighted clause byte range.
    pub preedit: Option<(String, Option<(usize, usize)>)>,
    pub on_submit: Option<Rc<dyn Fn(String)>>,
    /// Caret rectangle in logical window coordinates (updated on paint).
    pub cursor_rect: Rect,
    /// Horizontal scroll offset in logical pixels.
    pub scroll_x: f64,
}

impl TextInputState {
    pub fn selection(&self, text: &str) -> (usize, usize) {
        let c = clamp_boundary(text, self.cursor);
        let a = clamp_boundary(text, self.anchor);
        (a.min(c), a.max(c))
    }

    pub fn selected_text(&self) -> String {
        self.value.with_untracked(|text| {
            let (s, e) = self.selection(text);
            text[s..e].to_owned()
        })
    }

    pub fn insert_text(&mut self, insert: &str) {
        let mut next_cursor = 0;
        self.value.update(|text| {
            let (s, e) = self.selection(text);
            text.replace_range(s..e, insert);
            next_cursor = s + insert.len();
        });
        self.cursor = next_cursor;
        self.anchor = next_cursor;
    }

    pub fn set_value(&mut self, new_value: String) {
        let len = new_value.len();
        self.value.set(new_value);
        self.cursor = len;
        self.anchor = len;
        self.preedit = None;
    }

    pub fn backspace(&mut self) {
        let mut next_cursor = self.cursor;
        self.value.update(|text| {
            let (s, e) = self.selection(text);
            if s != e {
                text.replace_range(s..e, "");
                next_cursor = s;
            } else if s > 0 {
                let prev = prev_char_boundary(text, s);
                text.replace_range(prev..s, "");
                next_cursor = prev;
            }
        });
        self.cursor = next_cursor;
        self.anchor = next_cursor;
    }

    pub fn delete_forward(&mut self) {
        let mut next_cursor = self.cursor;
        self.value.update(|text| {
            let (s, e) = self.selection(text);
            if s != e {
                text.replace_range(s..e, "");
                next_cursor = s;
            } else if e < text.len() {
                let next = next_char_boundary(text, e);
                text.replace_range(e..next, "");
                next_cursor = e;
            }
        });
        self.cursor = next_cursor;
        self.anchor = next_cursor;
    }

    pub fn move_left(&mut self, extend: bool) {
        let (cursor, anchor) = self.value.with_untracked(|text| {
            let (s, e) = self.selection(text);
            if !extend && s != e {
                (s, s)
            } else {
                let c = prev_char_boundary(text, clamp_boundary(text, self.cursor));
                (
                    c,
                    if extend {
                        clamp_boundary(text, self.anchor)
                    } else {
                        c
                    },
                )
            }
        });
        self.cursor = cursor;
        self.anchor = anchor;
    }

    pub fn move_right(&mut self, extend: bool) {
        let (cursor, anchor) = self.value.with_untracked(|text| {
            let (s, e) = self.selection(text);
            if !extend && s != e {
                (e, e)
            } else {
                let c = next_char_boundary(text, clamp_boundary(text, self.cursor));
                (
                    c,
                    if extend {
                        clamp_boundary(text, self.anchor)
                    } else {
                        c
                    },
                )
            }
        });
        self.cursor = cursor;
        self.anchor = anchor;
    }

    pub fn move_home(&mut self, extend: bool) {
        let anchor = if extend {
            self.value
                .with_untracked(|t| clamp_boundary(t, self.anchor))
        } else {
            0
        };
        self.cursor = 0;
        self.anchor = anchor;
    }

    pub fn move_end(&mut self, extend: bool) {
        let (len, anchor) = self.value.with_untracked(|t| {
            let len = t.len();
            (
                len,
                if extend {
                    clamp_boundary(t, self.anchor)
                } else {
                    len
                },
            )
        });
        self.cursor = len;
        self.anchor = anchor;
    }

    pub fn select_all(&mut self) {
        let len = self.value.with_untracked(String::len);
        self.anchor = 0;
        self.cursor = len;
    }
}

fn clamp_boundary(text: &str, mut idx: usize) -> usize {
    idx = idx.min(text.len());
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn prev_char_boundary(text: &str, idx: usize) -> usize {
    let idx = clamp_boundary(text, idx);
    if idx == 0 {
        return 0;
    }
    text[..idx].char_indices().next_back().map_or(0, |(i, _)| i)
}

fn next_char_boundary(text: &str, idx: usize) -> usize {
    let idx = clamp_boundary(text, idx);
    if idx >= text.len() {
        return text.len();
    }
    text[idx..]
        .chars()
        .next()
        .map_or(text.len(), |c| idx + c.len_utf8())
}

/// What a node draws besides its box.
pub(crate) enum Content {
    None,
    Text { text: String, style: TextStyle },
    TextInput(TextInputState),
}

/// Pointer and keyboard interaction state for interactive nodes.
pub(crate) struct Interaction {
    pub hovered: Signal<bool>,
    pub pressed: Signal<bool>,
    pub focused: Signal<bool>,
    pub focusable: bool,
    pub on_click: Option<Rc<dyn Fn()>>,
}

/// Animated visual properties. Setters animate after the first layout and snap before it.
pub(crate) struct Visual {
    pub frame: Animated<Rect>,
    pub background: Animated<Color>,
    pub gloss: bool,
    pub radius: Animated<f64>,
    pub elevation: Animated<f64>,
    pub shadow_tint: Color,
    pub border_width: f64,
    pub border_color: Animated<Color>,
    pub scale: Animated<f64>,
    pub lift: Animated<f64>,
    pub text_color: Animated<Color>,
    /// Focus ring visibility (`0.0` unfocused to `1.0` focused).
    pub focus_ring: Animated<f64>,
    /// How geometry (frame, radius, elevation, scale, lift) animates.
    pub motion: Motion,
}

impl Default for Visual {
    fn default() -> Self {
        let t = theme();
        Self {
            frame: Animated::new(Rect::default()).geometry(),
            background: Animated::new(Color::TRANSPARENT),
            gloss: false,
            radius: Animated::new(0.0).geometry(),
            elevation: Animated::new(0.0).geometry(),
            shadow_tint: t.ink,
            border_width: 0.0,
            border_color: Animated::new(Color::TRANSPARENT),
            scale: Animated::new(1.0).geometry(),
            lift: Animated::new(0.0).geometry(),
            text_color: Animated::new(t.ink),
            focus_ring: Animated::new(0.0),
            motion: Motion::Spring(t.layout_spring),
        }
    }
}

pub(crate) struct Node {
    pub layout: LayoutId,
    pub style: LayoutStyle,
    pub children: Vec<NodeId>,
    pub content: Content,
    pub visual: Visual,
    /// Whether this node has been through layout once (until then, values snap).
    pub laid_out: bool,
    pub interaction: Option<Interaction>,
    /// Semantic role for accessibility.
    pub role: Option<Role>,
    /// Explicit accessibility label (overrides inferred label).
    pub a11y_label: Option<String>,
    /// Whether pressing on this node (when not hitting an interactive child) drags the window.
    pub draggable: bool,
}

impl Node {
    /// Animates `anim` to `value`, or snaps if the node hasn't been shown yet.
    pub fn animate<T: crate::motion::Animatable>(laid_out: bool, anim: &mut Animated<T>, value: T) {
        if laid_out {
            anim.set_target(value);
        } else {
            anim.snap(value);
        }
    }
}

pub(crate) struct Tree {
    pub nodes: SlotMap<NodeId, Node>,
    pub layout: LayoutTree,
    pub root: Option<NodeId>,
    pub needs_layout: bool,
    /// Something changed since the last frame.
    pub needs_frame: bool,
    /// Wakes the shell event loop when a reactive effect dirties the tree outside input.
    pub waker: Option<WakeHandle>,
}

impl Tree {
    fn new() -> Self {
        Self {
            nodes: SlotMap::with_key(),
            layout: LayoutTree::new(),
            root: None,
            needs_layout: true,
            needs_frame: true,
            waker: None,
        }
    }

    /// Marks the tree dirty and wakes the event loop if attached.
    pub fn mark_needs_frame(&mut self) {
        self.needs_frame = true;
        if let Some(waker) = &self.waker {
            waker.wake();
        }
    }

    pub fn add(&mut self, style: LayoutStyle, content: Content) -> NodeId {
        let measured = matches!(content, Content::Text { .. } | Content::TextInput(_));
        self.nodes.insert_with_key(|id| {
            let layout = if measured {
                self.layout.add_leaf(style, id.data().as_ffi())
            } else {
                self.layout.add(style)
            };
            Node {
                layout,
                style,
                children: Vec::new(),
                content,
                visual: Visual::default(),
                laid_out: false,
                interaction: None,
                role: None,
                a11y_label: None,
                draggable: false,
            }
        })
    }

    pub fn set_children(&mut self, parent: NodeId, children: Vec<NodeId>) {
        let layout_children: Vec<LayoutId> =
            children.iter().map(|c| self.nodes[*c].layout).collect();
        self.layout
            .set_children(self.nodes[parent].layout, &layout_children);
        self.nodes[parent].children = children;
        self.needs_layout = true;
    }

    /// Changes a node's layout style.
    pub fn update_style(&mut self, id: NodeId, f: impl FnOnce(&mut LayoutStyle)) {
        let node = &mut self.nodes[id];
        let before = node.style;
        f(&mut node.style);
        if node.style != before {
            self.layout.set_style(node.layout, node.style);
            self.needs_layout = true;
        }
    }

    /// Changes a text node's string.
    pub fn set_text(&mut self, id: NodeId, value: String) {
        let node = &mut self.nodes[id];
        if let Content::Text { text, .. } = &mut node.content
            && *text != value
        {
            *text = value;
            self.layout.mark_dirty(node.layout);
            self.needs_layout = true;
        }
    }

    /// Lays out the tree to fill `size` and retargets every node's frame.
    pub fn compute_layout(&mut self, size: Size, painter: &mut Painter<'_>) {
        let Some(root) = self.root else { return };
        self.update_style(root, |s| {
            s.width = rugkit_layout::Length::Px(size.width);
            s.height = rugkit_layout::Length::Px(size.height);
        });
        let nodes = &self.nodes;
        let root_layout = nodes[root].layout;
        self.layout.compute(
            root_layout,
            (size.width, size.height),
            |key, known, avail_w| {
                let id = NodeId::from(KeyData::from_ffi(key));
                match nodes.get(id).map(|n| &n.content) {
                    Some(Content::Text { text, style }) => {
                        let max = known.0.or(avail_w);
                        let s = painter.measure_text(text, max, style);
                        (
                            known.0.unwrap_or(s.width.ceil()),
                            known.1.unwrap_or(s.height.ceil()),
                        )
                    }
                    Some(Content::TextInput(state)) => {
                        let line_h = f64::from(state.style.size * state.style.line_height).ceil();
                        (known.0.unwrap_or(240.0), known.1.unwrap_or(line_h))
                    }
                    _ => (0.0, 0.0),
                }
            },
        );
        self.place(root, Point::ZERO);
        self.needs_layout = false;
    }

    fn place(&mut self, id: NodeId, parent_origin: Point) {
        let r = self.layout.rect(self.nodes[id].layout);
        let frame = Rect::from_xywh(
            parent_origin.x + r.x,
            parent_origin.y + r.y,
            r.width,
            r.height,
        );
        let node = &mut self.nodes[id];
        Node::animate(node.laid_out, &mut node.visual.frame, frame);
        node.laid_out = true;
        let children = node.children.clone();
        for child in children {
            self.place(child, frame.origin());
        }
    }

    /// Advances every animation. Returns `true` while anything is still moving.
    pub fn step(&mut self, dt: f64, reduce_motion: bool) -> bool {
        let color_motion = Motion::Spring(theme().color_spring);
        let mut moving = false;
        for node in self.nodes.values_mut() {
            let v = &mut node.visual;
            let geo = v.motion;
            let colors = if geo == Motion::Instant {
                Motion::Instant
            } else {
                color_motion
            };
            moving |= v.frame.step(dt, geo, reduce_motion);
            moving |= v.radius.step(dt, geo, reduce_motion);
            moving |= v.elevation.step(dt, geo, reduce_motion);
            moving |= v.scale.step(dt, geo, reduce_motion);
            moving |= v.lift.step(dt, geo, reduce_motion);
            moving |= v.background.step(dt, colors, reduce_motion);
            moving |= v.border_color.step(dt, colors, reduce_motion);
            moving |= v.text_color.step(dt, colors, reduce_motion);
            moving |= v.focus_ring.step(dt, colors, reduce_motion);
        }
        moving
    }

    pub fn paint(&mut self, painter: &mut Painter<'_>, focused: Option<NodeId>) {
        if let Some(root) = self.root {
            self.paint_node(root, painter, focused);
        }
    }

    fn paint_node(&mut self, id: NodeId, p: &mut Painter<'_>, focused: Option<NodeId>) {
        let active_theme = theme();
        let (
            frame,
            transform,
            radius,
            elevation,
            shadow_tint,
            bg,
            gloss,
            border_w,
            border_color,
            focus_ring,
            children,
        ) = {
            let node = &self.nodes[id];
            let v = &node.visual;
            let frame = v.frame.value();
            let transform = Transform::scale_about(v.scale.value(), frame.center())
                .then(Transform::translate(Vec2::new(0.0, v.lift.value())));
            (
                frame,
                transform,
                v.radius.value(),
                v.elevation.value(),
                v.shadow_tint,
                v.background.value(),
                v.gloss,
                v.border_width,
                v.border_color.value(),
                v.focus_ring.value(),
                node.children.clone(),
            )
        };

        p.with_transform(transform, |p| {
            if elevation > 0.05 {
                paint_elevation(p, frame, radius, elevation, shadow_tint, active_theme.ink);
            }
            if bg.a > 0.001 {
                if gloss {
                    let top = Point::new(frame.x0, frame.y0);
                    let bottom = Point::new(frame.x0, frame.y1);
                    let stops = [
                        (0.0, bg.mix(Color::WHITE.with_alpha(bg.a), 0.18)),
                        (1.0, bg.mix(Color::BLACK.with_alpha(bg.a), 0.04)),
                    ];
                    p.fill_rect(frame, radius, Brush::linear(top, bottom, stops));
                } else {
                    p.fill_rect(frame, radius, bg);
                }
            }
            if border_w > 0.0 {
                p.stroke_rect(
                    frame.inset(border_w / 2.0),
                    (radius - border_w / 2.0).max(0.0),
                    border_w,
                    border_color,
                );
            }
            if focus_ring > 0.01 {
                let expand = 3.0 * focus_ring;
                let ring_rect = frame.inset(-expand);
                let ring_radius = (radius + expand).max(0.0);
                p.stroke_rect(
                    ring_rect,
                    ring_radius,
                    3.0,
                    active_theme.accent.with_alpha(0.55 * focus_ring as f32),
                );
            }

            let is_focused = focused == Some(id);
            let node = &mut self.nodes[id];
            match &mut node.content {
                Content::None => {}
                Content::Text { text, style } => {
                    let style = style.clone().color(node.visual.text_color.value());
                    // Wrap at the target width so text doesn't reflow while the frame animates.
                    let width = node.visual.frame.target().width() + 0.5;
                    p.text_wrapped(text, frame.origin(), width, &style);
                }
                Content::TextInput(state) => {
                    let pad = node.style.padding;
                    let inner = Rect::from_xywh(
                        frame.x0 + pad.left,
                        frame.y0 + pad.top,
                        (frame.width() - pad.left - pad.right).max(1.0),
                        (frame.height() - pad.top - pad.bottom).max(1.0),
                    );
                    let text_color = node.visual.text_color.value();
                    let style = state.style.clone().color(text_color);
                    let line_h = f64::from(style.size * style.line_height);
                    let text_y = inner.y0 + ((inner.height() - line_h) * 0.5).max(0.0);
                    let value = state.value.get_untracked();

                    // Build display string (value + inline IME preedit at cursor).
                    let cursor = clamp_boundary(&value, state.cursor);
                    let (sel_start, sel_end) = state.selection(&value);
                    let (display, preedit_span) = if let Some((pre, clause)) = &state.preedit
                        && !pre.is_empty()
                    {
                        let mut s = String::with_capacity(value.len() + pre.len());
                        s.push_str(&value[..sel_start]);
                        let start = s.len();
                        s.push_str(pre);
                        let end = s.len();
                        s.push_str(&value[sel_end..]);
                        (s, Some((start, end, *clause)))
                    } else {
                        (value.clone(), None)
                    };

                    let caret_byte = match preedit_span {
                        Some((start, end, Some((_, c_end)))) => (start + c_end).min(end),
                        Some((_, end, None)) => end,
                        None => cursor,
                    };
                    let caret_x_raw = p.text_x_for_byte(&display, caret_byte, &style);

                    // Keep caret inside the visible horizontal bounds.
                    if caret_x_raw - state.scroll_x > inner.width() - 2.0 {
                        state.scroll_x = (caret_x_raw - inner.width() + 6.0).max(0.0);
                    } else if caret_x_raw - state.scroll_x < 0.0 {
                        state.scroll_x = caret_x_raw.max(0.0);
                    }
                    let origin_x = inner.x0 - state.scroll_x;
                    state.cursor_rect = Rect::from_xywh(
                        (origin_x + caret_x_raw).clamp(inner.x0, inner.x1),
                        text_y,
                        2.0,
                        line_h,
                    );

                    p.with_clip(inner, 4.0, |p| {
                        // Selection highlight (when no IME preedit is active).
                        if is_focused && preedit_span.is_none() && sel_start != sel_end {
                            let sx0 = origin_x + p.text_x_for_byte(&display, sel_start, &style);
                            let sx1 = origin_x + p.text_x_for_byte(&display, sel_end, &style);
                            p.fill_rect(
                                Rect::from_xywh(sx0, text_y, (sx1 - sx0).max(2.0), line_h),
                                4.0,
                                active_theme.accent.with_alpha(0.24),
                            );
                        }

                        if display.is_empty() {
                            if !state.placeholder.is_empty() {
                                let ph_style = style.clone().color(active_theme.ink_soft);
                                p.text(&state.placeholder, Point::new(origin_x, text_y), &ph_style);
                            }
                        } else {
                            p.text(&display, Point::new(origin_x, text_y), &style);
                        }

                        // IME preedit underline + stronger active clause underline.
                        if let Some((start, end, clause)) = preedit_span {
                            let ux0 = origin_x + p.text_x_for_byte(&display, start, &style);
                            let ux1 = origin_x + p.text_x_for_byte(&display, end, &style);
                            let uy = text_y + line_h - 2.0;
                            p.fill_rect(
                                Rect::from_xywh(ux0, uy, (ux1 - ux0).max(2.0), 1.5),
                                0.5,
                                text_color.with_alpha(0.45),
                            );
                            if let Some((cs, ce)) = clause
                                && cs < ce
                            {
                                let cx0 = origin_x
                                    + p.text_x_for_byte(&display, (start + cs).min(end), &style);
                                let cx1 = origin_x
                                    + p.text_x_for_byte(&display, (start + ce).min(end), &style);
                                p.fill_rect(
                                    Rect::from_xywh(cx0, uy - 0.5, (cx1 - cx0).max(2.0), 2.5),
                                    1.0,
                                    active_theme.accent,
                                );
                            }
                        }

                        // Caret bar when focused and no selection range.
                        if is_focused && (preedit_span.is_some() || sel_start == sel_end) {
                            let cx = origin_x + caret_x_raw;
                            p.fill_rect(
                                Rect::from_xywh(cx, text_y + 1.0, 2.0, (line_h - 2.0).max(4.0)),
                                1.0,
                                active_theme.accent,
                            );
                        }
                    });
                }
            }

            for child in children {
                self.paint_node(child, p, focused);
            }
        });
    }

    /// The topmost interactive node under `point` (using target frames).
    pub fn hit_test(&self, point: Point) -> Option<NodeId> {
        self.root.and_then(|r| self.hit(r, point))
    }

    fn hit(&self, id: NodeId, p: Point) -> Option<NodeId> {
        let node = &self.nodes[id];
        let f = node.visual.frame.target();
        if p.x < f.x0 || p.x > f.x1 || p.y < f.y0 || p.y > f.y1 {
            return None;
        }
        for child in node.children.iter().rev() {
            if let Some(hit) = self.hit(*child, p) {
                return Some(hit);
            }
        }
        node.interaction.as_ref().map(|_| id)
    }

    /// True if `point` falls inside a `.draggable()` region and not on an interactive widget.
    pub fn is_draggable_at(&self, point: Point) -> bool {
        if self.hit_test(point).is_some() {
            return false;
        }
        self.root.is_some_and(|r| self.hit_draggable(r, point))
    }

    fn hit_draggable(&self, id: NodeId, p: Point) -> bool {
        let node = &self.nodes[id];
        let f = node.visual.frame.target();
        if p.x < f.x0 || p.x > f.x1 || p.y < f.y0 || p.y > f.y1 {
            return false;
        }
        for child in node.children.iter().rev() {
            if self.hit_draggable(*child, p) {
                return true;
            }
        }
        node.draggable
    }

    /// All focusable nodes in depth-first traversal order.
    pub fn focusable_nodes(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        if let Some(root) = self.root {
            self.collect_focusable(root, &mut out);
        }
        out
    }

    fn collect_focusable(&self, id: NodeId, out: &mut Vec<NodeId>) {
        let node = &self.nodes[id];
        if node.interaction.as_ref().is_some_and(|i| i.focusable) {
            out.push(id);
        }
        for &child in &node.children {
            self.collect_focusable(child, out);
        }
    }

    /// Builds a snapshot of the accessibility tree for AccessKit and the headless harness.
    pub fn a11y_tree(&self, focused: Option<NodeId>, window_size: Size) -> A11yTree {
        let mut nodes = Vec::new();
        let mut root_children = Vec::new();
        if let Some(root) = self.root {
            self.collect_a11y(root, &mut nodes, &mut root_children);
        }
        let mut win = A11yNode::new(
            WINDOW_A11Y_ID,
            Role::Window,
            Rect::from_origin_size(Point::ZERO, window_size),
        );
        win.children = root_children;
        nodes.push(win);

        let focus = focused
            .filter(|id| nodes.iter().any(|n| n.id == id.to_a11y()))
            .map_or(WINDOW_A11Y_ID, NodeId::to_a11y);

        A11yTree {
            root: WINDOW_A11Y_ID,
            focus,
            nodes,
        }
    }

    fn collect_a11y(&self, id: NodeId, out: &mut Vec<A11yNode>, parent_children: &mut Vec<A11yId>) {
        let node = &self.nodes[id];
        let bounds = node.visual.frame.target();
        let focusable = node.interaction.as_ref().is_some_and(|i| i.focusable);

        match node.role {
            Some(Role::Button) => {
                let mut a = A11yNode::new(id.to_a11y(), Role::Button, bounds);
                a.label = node
                    .a11y_label
                    .clone()
                    .or_else(|| self.first_descendant_text(id));
                a.focusable = focusable;
                parent_children.push(a.id);
                out.push(a);
            }
            Some(Role::TextInput) => {
                let mut a = A11yNode::new(id.to_a11y(), Role::TextInput, bounds);
                if let Content::TextInput(state) = &node.content {
                    a.label = node.a11y_label.clone().or_else(|| {
                        (!state.placeholder.is_empty()).then(|| state.placeholder.clone())
                    });
                    a.value = Some(state.value.get_untracked());
                }
                a.focusable = focusable;
                parent_children.push(a.id);
                out.push(a);
            }
            Some(Role::Label) => {
                let mut a = A11yNode::new(id.to_a11y(), Role::Label, bounds);
                let text = match &node.content {
                    Content::Text { text, .. } => Some(text.clone()),
                    _ => None,
                };
                a.label = node.a11y_label.clone().or(text);
                parent_children.push(a.id);
                out.push(a);
            }
            Some(role) => {
                let mut children = Vec::new();
                for &child in &node.children {
                    self.collect_a11y(child, out, &mut children);
                }
                let mut a = A11yNode::new(id.to_a11y(), role, bounds);
                a.label = node.a11y_label.clone();
                a.focusable = focusable;
                a.children = children;
                parent_children.push(a.id);
                out.push(a);
            }
            None => {
                // Anonymous layout container: flatten children directly into parent.
                for &child in &node.children {
                    self.collect_a11y(child, out, parent_children);
                }
            }
        }
    }

    fn first_descendant_text(&self, id: NodeId) -> Option<String> {
        let node = &self.nodes[id];
        if let Content::Text { text, .. } = &node.content {
            return Some(text.clone());
        }
        for &child in &node.children {
            if let Some(t) = self.first_descendant_text(child) {
                return Some(t);
            }
        }
        None
    }
}

/// Two layered shadows for an elevation level: a tight contact shadow and a wide, tinted
/// ambient one.
fn paint_elevation(p: &mut Painter<'_>, frame: Rect, radius: f64, e: f64, tint: Color, ink: Color) {
    p.shadow(
        frame,
        radius,
        &Shadow::new(ink.with_alpha(0.10), Vec2::new(0.0, e * 0.35), e * 1.2),
    );
    let strength = (e / 8.0).min(1.5) as f32;
    p.shadow(
        frame,
        radius,
        &Shadow::new(
            tint.with_alpha(0.22 * strength),
            Vec2::new(0.0, e * 1.6),
            e * 4.0,
        )
        .with_spread(-e * 0.4),
    );
}

/// Shared handle to the tree, captured by the effects that keep nodes in sync with signals.
#[derive(Clone)]
pub(crate) struct TreeHandle(Rc<RefCell<Tree>>);

impl TreeHandle {
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(Tree::new())))
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut Tree) -> R) -> R {
        let mut tree = self.0.try_borrow_mut().unwrap_or_else(|_| {
            panic!(
                "rugkit: the view tree was modified while it was being laid out or drawn. \
                 Don't write signals from drawing code."
            )
        });
        f(&mut tree)
    }
}
