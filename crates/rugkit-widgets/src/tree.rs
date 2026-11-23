//! The retained node tree: layout, animation stepping, painting and hit-testing.

use std::cell::RefCell;
use std::rc::Rc;

use rugkit_layout::{LayoutId, LayoutStyle, LayoutTree};
use rugkit_reactive::Signal;
use rugkit_render::{Brush, Color, Painter, Point, Rect, Shadow, Size, TextStyle, Transform, Vec2};
use slotmap::{Key, KeyData, SlotMap};

use crate::motion::{Animated, Motion};
use crate::theme::theme;

slotmap::new_key_type! {
    /// Identifies a node in the view tree.
    pub struct NodeId;
}

/// What a node draws besides its box.
pub(crate) enum Content {
    None,
    Text { text: String, style: TextStyle },
}

/// Pointer interaction state for interactive nodes (buttons and so on).
pub(crate) struct Interaction {
    pub hovered: Signal<bool>,
    pub pressed: Signal<bool>,
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
    /// How geometry (frame, radius, elevation, scale, lift) animates.
    pub motion: Motion,
}

impl Default for Visual {
    fn default() -> Self {
        Self {
            frame: Animated::new(Rect::default()).geometry(),
            background: Animated::new(Color::TRANSPARENT),
            gloss: false,
            radius: Animated::new(0.0).geometry(),
            elevation: Animated::new(0.0).geometry(),
            shadow_tint: theme().ink,
            border_width: 0.0,
            border_color: Animated::new(Color::TRANSPARENT),
            scale: Animated::new(1.0).geometry(),
            lift: Animated::new(0.0).geometry(),
            text_color: Animated::new(theme().ink),
            motion: Motion::Spring(theme().layout_spring),
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
}

impl Tree {
    fn new() -> Self {
        Self {
            nodes: SlotMap::with_key(),
            layout: LayoutTree::new(),
            root: None,
            needs_layout: true,
            needs_frame: true,
        }
    }

    pub fn add(&mut self, style: LayoutStyle, content: Content) -> NodeId {
        let measured = matches!(content, Content::Text { .. });
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
        }
        moving
    }

    pub fn paint(&self, painter: &mut Painter<'_>) {
        if let Some(root) = self.root {
            self.paint_node(root, painter);
        }
    }

    fn paint_node(&self, id: NodeId, p: &mut Painter<'_>) {
        let node = &self.nodes[id];
        let v = &node.visual;
        let frame = v.frame.value();
        let transform = Transform::scale_about(v.scale.value(), frame.center())
            .then(Transform::translate(Vec2::new(0.0, v.lift.value())));
        p.with_transform(transform, |p| {
            let radius = v.radius.value();
            let elevation = v.elevation.value();
            if elevation > 0.05 {
                paint_elevation(p, frame, radius, elevation, v.shadow_tint);
            }
            let bg = v.background.value();
            if bg.a > 0.001 {
                if v.gloss {
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
            if v.border_width > 0.0 {
                let w = v.border_width;
                p.stroke_rect(
                    frame.inset(w / 2.0),
                    (radius - w / 2.0).max(0.0),
                    w,
                    v.border_color.value(),
                );
            }
            if let Content::Text { text, style } = &node.content {
                let style = style.clone().color(v.text_color.value());
                // Wrap at the target width so text doesn't reflow while the frame animates.
                let width = v.frame.target().width() + 0.5;
                p.text_wrapped(text, frame.origin(), width, &style);
            }
            for child in &node.children {
                self.paint_node(*child, p);
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
}

/// Two layered shadows for an elevation level: a tight contact shadow and a wide, tinted
/// ambient one.
fn paint_elevation(p: &mut Painter<'_>, frame: Rect, radius: f64, e: f64, tint: Color) {
    let ink = theme().ink;
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
