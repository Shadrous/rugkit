//! The retained node tree: layout, animation stepping, painting, hit-testing and accessibility.

use std::cell::RefCell;
use std::rc::Rc;

use rugkit_layout::{LayoutId, LayoutStyle, LayoutTree};
use rugkit_reactive::Signal;
use rugkit_render::{Brush, Color, Painter, Point, Rect, Shadow, Size, TextStyle, Transform, Vec2};
use rugkit_shell::{
    A11yId, A11yNode, A11yTree, Key as ShellKey, Modifiers, PointerButton, Role, WakeHandle,
};
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

/// Pointer event kind delivered to rich interactive widgets (`Canvas`, `Table`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum PointerKind {
    Move,
    Down(PointerButton),
    Up(PointerButton),
    Leave,
}

/// Custom draw callback for icons, images, checkboxes, toggles, sliders, canvas and tables.
pub(crate) type CustomPaint = Rc<dyn Fn(&mut Painter<'_>, Rect, Color)>;
pub(crate) type DragHandler = Rc<dyn Fn(Point, Rect)>;
pub(crate) type PointerHandler = Rc<dyn Fn(PointerKind, Point, Rect, Modifiers)>;
pub(crate) type KeyHandler = Rc<dyn Fn(&ShellKey, Modifiers) -> bool>;
pub(crate) type ScrollHandler = Rc<dyn Fn(Vec2)>;
pub(crate) type CheckedGetter = Rc<dyn Fn() -> bool>;
pub(crate) type NumericGetter = Rc<dyn Fn() -> (f64, f64, f64)>;

/// What a node draws besides its box.
pub(crate) enum Content {
    None,
    Text { text: String, style: TextStyle },
    TextInput(TextInputState),
    Custom(CustomPaint),
}

/// Pointer and keyboard interaction state for interactive nodes.
pub(crate) struct Interaction {
    pub hovered: Signal<bool>,
    pub pressed: Signal<bool>,
    pub focused: Signal<bool>,
    pub focusable: bool,
    pub on_click: Option<Rc<dyn Fn()>>,
    pub on_drag: Option<DragHandler>,
    pub on_pointer: Option<PointerHandler>,
    pub on_key: Option<KeyHandler>,
    pub on_scroll: Option<ScrollHandler>,
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
    pub opacity: Animated<f64>,
    pub text_color: Animated<Color>,
    /// Focus ring visibility (`0.0` unfocused to `1.0` focused).
    pub focus_ring: Animated<f64>,
    /// Smooth vertical scroll offset for scroll containers.
    pub scroll_y: Animated<f64>,
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
            opacity: Animated::new(1.0),
            text_color: Animated::new(t.ink),
            focus_ring: Animated::new(0.0),
            scroll_y: Animated::new(0.0).geometry(),
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
    /// Live checked/toggled getter for checkboxes, switches and radios.
    pub a11y_checked: Option<CheckedGetter>,
    /// Live numeric `(value, min, max)` getter for sliders.
    pub a11y_numeric: Option<NumericGetter>,
    /// Whether pressing on this node (when not hitting an interactive child) drags the window.
    pub draggable: bool,
    /// Whether this widget is disabled (blocks interaction and dims visually).
    pub disabled: bool,
    /// Whether children are clipped to this node's frame (used by scroll containers).
    pub clip_content: bool,
    /// Maximum vertical scroll extent in logical pixels.
    pub scroll_max: f64,
    /// Optional hover tooltip text.
    pub tooltip: Option<String>,
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

    /// Removes all nodes and resets layout (preserving the attached `waker`), used on hot-reload.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.layout = LayoutTree::new();
        self.root = None;
        self.needs_layout = true;
        self.needs_frame = true;
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
                a11y_checked: None,
                a11y_numeric: None,
                draggable: false,
                disabled: false,
                clip_content: false,
                scroll_max: 0.0,
                tooltip: None,
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
        let clip_content = node.clip_content;
        for &child in &children {
            self.place(child, frame.origin());
        }
        if clip_content {
            let mut max_bottom = frame.y0;
            for &child in &children {
                let b = self.nodes[child].visual.frame.target().y1;
                if b > max_bottom {
                    max_bottom = b;
                }
            }
            let node = &mut self.nodes[id];
            let pad_b = node.style.padding.bottom;
            node.scroll_max = ((max_bottom + pad_b) - frame.y1).max(0.0);
            let clamped = node.visual.scroll_y.target().clamp(0.0, node.scroll_max);
            node.visual.scroll_y.set_target(clamped);
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
            moving |= v.opacity.step(dt, colors, reduce_motion);
            moving |= v.background.step(dt, colors, reduce_motion);
            moving |= v.border_color.step(dt, colors, reduce_motion);
            moving |= v.text_color.step(dt, colors, reduce_motion);
            moving |= v.focus_ring.step(dt, colors, reduce_motion);
            moving |= v.scroll_y.step(dt, colors, reduce_motion);
        }
        moving
    }

    pub fn paint(&mut self, painter: &mut Painter<'_>, focused: Option<NodeId>) {
        if let Some(root) = self.root {
            self.paint_node(root, painter, focused);
        }
    }

    fn paint_node(&mut self, id: NodeId, p: &mut Painter<'_>, focused: Option<NodeId>) {
        if self.nodes[id].style.hidden {
            return;
        }
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
            opacity,
            disabled,
            clip_content,
            scroll_y,
            scroll_max,
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
                v.opacity.value() as f32,
                node.disabled,
                node.clip_content,
                v.scroll_y.value(),
                node.scroll_max,
                node.children.clone(),
            )
        };
        let effective_opacity = if disabled { opacity * 0.48 } else { opacity };
        if effective_opacity <= 0.005 {
            return;
        }

        let draw_body = |this: &mut Self, p: &mut Painter<'_>| {
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
                let custom = {
                    let node = &mut this.nodes[id];
                    match &mut node.content {
                        Content::None => None,
                        Content::Custom(draw_fn) => {
                            Some((draw_fn.clone(), node.visual.text_color.value()))
                        }
                        Content::Text { text, style } => {
                            let style = style.clone().color(node.visual.text_color.value());
                            let width = node.visual.frame.target().width() + 0.5;
                            p.text_wrapped(text, frame.origin(), width, &style);
                            None
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

                            let cursor = clamp_boundary(&value, state.cursor);
                            let (sel_start, sel_end) = state.selection(&value);
                            let (display, preedit_span) = if let Some((pre, clause)) =
                                &state.preedit
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
                                if is_focused && preedit_span.is_none() && sel_start != sel_end {
                                    let sx0 =
                                        origin_x + p.text_x_for_byte(&display, sel_start, &style);
                                    let sx1 =
                                        origin_x + p.text_x_for_byte(&display, sel_end, &style);
                                    p.fill_rect(
                                        Rect::from_xywh(sx0, text_y, (sx1 - sx0).max(2.0), line_h),
                                        4.0,
                                        active_theme.accent.with_alpha(0.24),
                                    );
                                }

                                if display.is_empty() {
                                    if !state.placeholder.is_empty() {
                                        let ph_style = style.clone().color(active_theme.ink_soft);
                                        p.text(
                                            &state.placeholder,
                                            Point::new(origin_x, text_y),
                                            &ph_style,
                                        );
                                    }
                                } else {
                                    p.text(&display, Point::new(origin_x, text_y), &style);
                                }

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
                                            + p.text_x_for_byte(
                                                &display,
                                                (start + cs).min(end),
                                                &style,
                                            );
                                        let cx1 = origin_x
                                            + p.text_x_for_byte(
                                                &display,
                                                (start + ce).min(end),
                                                &style,
                                            );
                                        p.fill_rect(
                                            Rect::from_xywh(
                                                cx0,
                                                uy - 0.5,
                                                (cx1 - cx0).max(2.0),
                                                2.5,
                                            ),
                                            1.0,
                                            active_theme.accent,
                                        );
                                    }
                                }

                                if is_focused && (preedit_span.is_some() || sel_start == sel_end) {
                                    let cx = origin_x + caret_x_raw;
                                    p.fill_rect(
                                        Rect::from_xywh(
                                            cx,
                                            text_y + 1.0,
                                            2.0,
                                            (line_h - 2.0).max(4.0),
                                        ),
                                        1.0,
                                        active_theme.accent,
                                    );
                                }
                            });
                            None
                        }
                    }
                };
                if let Some((draw_fn, color)) = custom {
                    draw_fn(p, frame, color);
                }

                if clip_content {
                    p.with_clip(frame, radius, |p| {
                        p.with_transform(Transform::translate(Vec2::new(0.0, -scroll_y)), |p| {
                            for child in &children {
                                this.paint_node(*child, p, focused);
                            }
                        });
                        // Draw subtle scrollbar thumb when content overflows.
                        if scroll_max > 1.0 && frame.height() > 36.0 {
                            let content_h = frame.height() + scroll_max;
                            let thumb_h = ((frame.height() / content_h) * frame.height())
                                .clamp(24.0, frame.height() - 8.0);
                            let frac = (scroll_y / scroll_max).clamp(0.0, 1.0);
                            let thumb_y = frame.y0 + 4.0 + frac * (frame.height() - thumb_h - 8.0);
                            p.fill_rect(
                                Rect::from_xywh(frame.x1 - 8.0, thumb_y, 4.0, thumb_h),
                                2.0,
                                active_theme.ink.with_alpha(0.24),
                            );
                        }
                    });
                } else {
                    for child in &children {
                        this.paint_node(*child, p, focused);
                    }
                }
            });
        };

        if effective_opacity < 0.995 {
            let bounds = frame.inset(-40.0);
            p.with_opacity(effective_opacity, bounds, |p| draw_body(self, p));
        } else {
            draw_body(self, p);
        }
    }

    /// Draws coloured layout-bounds outlines for every visible node (used by the inspector).
    pub fn paint_inspector_bounds(&self, p: &mut Painter<'_>) {
        if let Some(root) = self.root {
            self.paint_bounds_rec(root, p, 0);
        }
    }

    fn paint_bounds_rec(&self, id: NodeId, p: &mut Painter<'_>, depth: usize) {
        let node = &self.nodes[id];
        if node.style.hidden {
            return;
        }
        let palette = [
            Color::from_hex(0x00D1FF),
            Color::from_hex(0xFF4D94),
            Color::from_hex(0x00E5A0),
            Color::from_hex(0xFFB74D),
        ];
        let color = palette[depth % palette.len()].with_alpha(0.55);
        p.stroke_rect(node.visual.frame.value(), 2.0, 1.0, color);
        for &child in &node.children {
            self.paint_bounds_rec(child, p, depth + 1);
        }
    }

    /// The topmost interactive, enabled, visible node under `point` (accounting for scroll offsets).
    pub fn hit_test(&self, point: Point) -> Option<NodeId> {
        self.root.and_then(|r| self.hit(r, point))
    }

    fn hit(&self, id: NodeId, p: Point) -> Option<NodeId> {
        let node = &self.nodes[id];
        if node.style.hidden {
            return None;
        }
        let f = node.visual.frame.target();
        if p.x < f.x0 || p.x > f.x1 || p.y < f.y0 || p.y > f.y1 {
            return None;
        }
        let child_p = if node.clip_content {
            Point::new(p.x, p.y + node.visual.scroll_y.value())
        } else {
            p
        };
        for child in node.children.iter().rev() {
            if let Some(hit) = self.hit(*child, child_p) {
                return Some(hit);
            }
        }
        if !node.disabled && node.interaction.is_some() {
            Some(id)
        } else {
            None
        }
    }

    /// Finds the innermost scroll container under `point`.
    pub fn scroll_target_at(&self, point: Point) -> Option<NodeId> {
        self.root.and_then(|r| self.hit_scroll(r, point))
    }

    fn hit_scroll(&self, id: NodeId, p: Point) -> Option<NodeId> {
        let node = &self.nodes[id];
        if node.style.hidden {
            return None;
        }
        let f = node.visual.frame.target();
        if p.x < f.x0 || p.x > f.x1 || p.y < f.y0 || p.y > f.y1 {
            return None;
        }
        let child_p = if node.clip_content {
            Point::new(p.x, p.y + node.visual.scroll_y.value())
        } else {
            p
        };
        for child in node.children.iter().rev() {
            if let Some(hit) = self.hit_scroll(*child, child_p) {
                return Some(hit);
            }
        }
        if node.clip_content
            || node
                .interaction
                .as_ref()
                .is_some_and(|i| i.on_scroll.is_some())
        {
            Some(id)
        } else {
            None
        }
    }

    /// Finds the innermost node with a tooltip under `point`, returning `(tooltip_text, anchor_rect)`.
    pub fn tooltip_at(&self, point: Point) -> Option<(String, Rect)> {
        self.root.and_then(|r| self.hit_tooltip(r, point))
    }

    fn hit_tooltip(&self, id: NodeId, p: Point) -> Option<(String, Rect)> {
        let node = &self.nodes[id];
        if node.style.hidden {
            return None;
        }
        let f = node.visual.frame.target();
        if p.x < f.x0 || p.x > f.x1 || p.y < f.y0 || p.y > f.y1 {
            return None;
        }
        let child_p = if node.clip_content {
            Point::new(p.x, p.y + node.visual.scroll_y.value())
        } else {
            p
        };
        for child in node.children.iter().rev() {
            if let Some(hit) = self.hit_tooltip(*child, child_p) {
                return Some(hit);
            }
        }
        node.tooltip.as_ref().map(|t| (t.clone(), f))
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
        if node.style.hidden {
            return false;
        }
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

    /// All visible, enabled, focusable nodes in depth-first traversal order.
    pub fn focusable_nodes(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        if let Some(root) = self.root {
            self.collect_focusable(root, &mut out);
        }
        out
    }

    fn collect_focusable(&self, id: NodeId, out: &mut Vec<NodeId>) {
        let node = &self.nodes[id];
        if node.style.hidden || node.disabled {
            return;
        }
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
        if node.style.hidden {
            return;
        }
        let bounds = node.visual.frame.target();
        let focusable = !node.disabled && node.interaction.as_ref().is_some_and(|i| i.focusable);

        match node.role {
            Some(
                role @ (Role::Button
                | Role::Checkbox
                | Role::Switch
                | Role::RadioButton
                | Role::ComboBox
                | Role::Tab),
            ) => {
                let mut a = A11yNode::new(id.to_a11y(), role, bounds);
                a.label = node
                    .a11y_label
                    .clone()
                    .or_else(|| self.first_descendant_text(id));
                a.focusable = focusable;
                a.disabled = node.disabled;
                if let Some(chk) = &node.a11y_checked {
                    a.checked = Some(chk());
                }
                parent_children.push(a.id);
                out.push(a);
            }
            Some(Role::Slider) => {
                let mut a = A11yNode::new(id.to_a11y(), Role::Slider, bounds);
                a.label = node.a11y_label.clone();
                a.focusable = focusable;
                a.disabled = node.disabled;
                if let Some(num) = &node.a11y_numeric {
                    let (val, min, max) = num();
                    a.numeric_value = Some(val);
                    a.numeric_range = Some((min, max));
                }
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
                a.disabled = node.disabled;
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
                a.disabled = node.disabled;
                a.children = children;
                parent_children.push(a.id);
                out.push(a);
            }
            None => {
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
/// ambient one. Blur radii are capped so large cards stay comfortably inside the 8ms GPU budget.
fn paint_elevation(p: &mut Painter<'_>, frame: Rect, radius: f64, e: f64, tint: Color, ink: Color) {
    p.shadow(
        frame,
        radius,
        &Shadow::new(
            ink.with_alpha(0.10),
            Vec2::new(0.0, (e * 0.3).min(4.0)),
            (e * 0.9).min(8.0),
        ),
    );
    let strength = (e / 8.0).min(1.4) as f32;
    p.shadow(
        frame,
        radius,
        &Shadow::new(
            tint.with_alpha(0.20 * strength),
            Vec2::new(0.0, (e * 1.1).min(14.0)),
            (e * 2.2).min(20.0),
        )
        .with_spread(-(e * 0.35).min(4.0)),
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
