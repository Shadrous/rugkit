//! Reactive [`canvas`] drawing surface with automatic signal tracking and [`HitRegions`]
//! hit-testing helpers.

use std::rc::Rc;

use rugkit_layout::{LayoutStyle, Length};
use rugkit_reactive::{Tracker, signal};
use rugkit_render::{Painter, Point, Rect};
use rugkit_shell::{PointerButton, Role};

use crate::common::{Common, common_modifiers};
use crate::tree::{Content, Interaction, NodeId, PointerKind};
use crate::view::{BuildCx, View};

/// A lightweight hit-testing helper for [`canvas`] drawings.
///
/// Record interactive shapes (rectangles or circles in canvas-local coordinates) as you draw
/// or lay out a chart, then call [`HitRegions::hit_test`] from `.on_hover` / `.on_click` /
/// `.on_pointer_down` to find the topmost shape under the pointer.
#[derive(Clone, Debug)]
pub struct HitRegions<T> {
    entries: Vec<(HitShape, T)>,
}

#[derive(Copy, Clone, Debug)]
enum HitShape {
    Rect(Rect),
    Circle(Point, f64),
}

impl<T> Default for HitRegions<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> HitRegions<T> {
    /// Creates an empty hit-test region list.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Removes all recorded regions (typically called at the start of a draw pass).
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Registers a hit-testable rectangle associated with `id`.
    pub fn push_rect(&mut self, id: T, rect: Rect) {
        self.entries.push((HitShape::Rect(rect), id));
    }

    /// Registers a hit-testable circle associated with `id`.
    pub fn push_circle(&mut self, id: T, center: Point, radius: f64) {
        self.entries.push((HitShape::Circle(center, radius), id));
    }

    /// Returns the topmost region containing `point` (last pushed wins), or `None`.
    pub fn hit_test(&self, point: Point) -> Option<&T> {
        for (shape, id) in self.entries.iter().rev() {
            let hit = match *shape {
                HitShape::Rect(r) => {
                    point.x >= r.x0 && point.x <= r.x1 && point.y >= r.y0 && point.y <= r.y1
                }
                HitShape::Circle(c, radius) => {
                    let dx = point.x - c.x;
                    let dy = point.y - c.y;
                    dx * dx + dy * dy <= radius * radius
                }
            };
            if hit {
                return Some(id);
            }
        }
        None
    }
}

type DrawFn = Rc<dyn Fn(&mut Painter<'_>, Rect)>;
type HoverHandler = Rc<dyn Fn(Option<Point>)>;
type ClickHandler = Rc<dyn Fn(Point)>;
type PointerButtonHandler = Rc<dyn Fn(Point, PointerButton)>;

/// A reactive custom-drawing widget. Created with [`canvas`].
///
/// The draw closure runs with a [`Painter`] and the canvas's bounding [`Rect`], and
/// automatically re-runs whenever any signal it reads changes. Pointer and hover callbacks
/// receive coordinates relative to the canvas's top-left corner (`(0, 0)`).
pub struct CanvasView {
    draw: DrawFn,
    on_hover: Option<HoverHandler>,
    on_click: Option<ClickHandler>,
    on_pointer_down: Option<PointerButtonHandler>,
    on_pointer_up: Option<PointerButtonHandler>,
    common: Common,
}

impl std::fmt::Debug for CanvasView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CanvasView").finish_non_exhaustive()
    }
}

/// Creates a reactive [`CanvasView`] drawn by `draw(painter, bounds)`.
///
/// Any [`rugkit_reactive::Signal`] or [`rugkit_reactive::Memo`] read inside `draw` is tracked
/// automatically: when it changes, the canvas schedules a redraw without rebuilding the tree.
pub fn canvas(draw: impl Fn(&mut Painter<'_>, Rect) + 'static) -> CanvasView {
    CanvasView {
        draw: Rc::new(draw),
        on_hover: None,
        on_click: None,
        on_pointer_down: None,
        on_pointer_up: None,
        common: Common::default(),
    }
}

impl CanvasView {
    /// Called with the canvas-local pointer position on move, or `None` when the pointer leaves.
    pub fn on_hover(mut self, f: impl Fn(Option<Point>) + 'static) -> Self {
        self.on_hover = Some(Rc::new(f));
        self
    }

    /// Called with the canvas-local pointer position on primary click release.
    pub fn on_click(mut self, f: impl Fn(Point) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// Called with canvas-local position and button on pointer press.
    pub fn on_pointer_down(mut self, f: impl Fn(Point, PointerButton) + 'static) -> Self {
        self.on_pointer_down = Some(Rc::new(f));
        self
    }

    /// Called with canvas-local position and button on pointer release.
    pub fn on_pointer_up(mut self, f: impl Fn(Point, PointerButton) + 'static) -> Self {
        self.on_pointer_up = Some(Rc::new(f));
        self
    }
}

common_modifiers!(CanvasView);

impl View for CanvasView {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let tree_for_dirty = cx.tree.clone();
        let tracker = Tracker::new(move || {
            tree_for_dirty.with(|t| t.mark_needs_frame());
        });

        let draw = self.draw;
        let id = cx.node(
            LayoutStyle {
                width: Length::Px(320.0),
                height: Length::Px(200.0),
                ..Default::default()
            },
            Content::Custom(Rc::new(move |p, rect, _| {
                p.with_clip(rect, 0.0, |p| {
                    tracker.track(|| draw(p, rect));
                });
            })),
        );

        let has_pointer = self.on_hover.is_some()
            || self.on_click.is_some()
            || self.on_pointer_down.is_some()
            || self.on_pointer_up.is_some();

        cx.with_tree(|t| {
            let n = &mut t.nodes[id];
            n.role = Some(Role::Image);
            if has_pointer {
                let hovered = signal(false);
                let pressed = signal(false);
                let focused = signal(false);
                let on_hover = self.on_hover;
                let on_click = self.on_click;
                let on_down = self.on_pointer_down;
                let on_up = self.on_pointer_up;
                n.interaction = Some(Interaction {
                    hovered,
                    pressed,
                    focused,
                    focusable: false,
                    on_click: None,
                    on_drag: None,
                    on_pointer: Some(Rc::new(move |kind, pt, rect, _| {
                        let local = Point::new(pt.x - rect.x0, pt.y - rect.y0);
                        match kind {
                            PointerKind::Move => {
                                if let Some(h) = &on_hover {
                                    h(Some(local));
                                }
                            }
                            PointerKind::Leave => {
                                if let Some(h) = &on_hover {
                                    h(None);
                                }
                            }
                            PointerKind::Down(btn) => {
                                if let Some(d) = &on_down {
                                    d(local, btn);
                                }
                            }
                            PointerKind::Up(btn) => {
                                if let Some(u) = &on_up {
                                    u(local, btn);
                                }
                                if btn == PointerButton::Primary
                                    && let Some(c) = &on_click
                                    && pt.x >= rect.x0
                                    && pt.x <= rect.x1
                                    && pt.y >= rect.y0
                                    && pt.y <= rect.y1
                                {
                                    c(local);
                                }
                            }
                        }
                    })),
                    on_key: None,
                    on_scroll: None,
                });
            }
        });
        self.common.apply(cx, id);
        id
    }
}
