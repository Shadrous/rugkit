//! [`Ui`]: hosts a view tree in a window, routes input and runs the frame loop.

use rugkit_layout::{Align, Justify, LayoutStyle, Length};
use rugkit_reactive::{Owner, Signal, batch};
use rugkit_render::{Painter, Size};
use rugkit_shell::{App, FrameInfo, InputEvent, PointerButton};

use crate::tree::{Content, NodeId, TreeHandle};
use crate::view::{BuildCx, View};

/// A view tree hosted in a window. Implements [`App`], so it can be passed to
/// `rugkit_shell::run` (the `rugkit` facade's `launch` does this for you).
///
/// Frames are drawn only while something changes or animates. An idle window costs nothing.
pub struct Ui {
    tree: TreeHandle,
    owner: Owner,
    hovered: Option<NodeId>,
    captured: Option<NodeId>,
    animating: bool,
    last_size: Option<Size>,
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
            let mut cx = BuildCx { tree: tree.clone() };
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
            animating: false,
            last_size: None,
        }
    }

    fn signals(&self, id: Option<NodeId>) -> Option<(Signal<bool>, Signal<bool>)> {
        let id = id?;
        self.tree.with(|t| {
            t.nodes
                .get(id)
                .and_then(|n| n.interaction.as_ref())
                .map(|i| (i.hovered, i.pressed))
        })
    }

    fn set_hover(&mut self, target: Option<NodeId>) {
        if target == self.hovered {
            return;
        }
        let old = self.signals(self.hovered);
        let new = self.signals(target);
        self.hovered = target;
        batch(|| {
            if let Some((h, _)) = old {
                h.set_if_changed(false);
            }
            if let Some((h, _)) = new {
                h.set_if_changed(true);
            }
        });
    }
}

impl App for Ui {
    fn event(&mut self, event: &InputEvent) {
        match *event {
            InputEvent::PointerMove(p) => {
                let target = self.tree.with(|t| t.hit_test(p));
                self.set_hover(target);
                // While a press is held, show the pressed look only while over the button.
                if let Some(captured) = self.captured
                    && let Some((_, pressed)) = self.signals(Some(captured))
                {
                    pressed.set_if_changed(target == Some(captured));
                }
            }
            InputEvent::PointerDown(p, PointerButton::Primary) => {
                let target = self.tree.with(|t| t.hit_test(p));
                self.set_hover(target);
                self.captured = target;
                if let Some((_, pressed)) = self.signals(target) {
                    pressed.set(true);
                }
            }
            InputEvent::PointerUp(p, PointerButton::Primary) => {
                let target = self.tree.with(|t| t.hit_test(p));
                let Some(captured) = self.captured.take() else {
                    return;
                };
                if let Some((_, pressed)) = self.signals(Some(captured)) {
                    pressed.set_if_changed(false);
                }
                if target == Some(captured) {
                    let handler = self.tree.with(|t| {
                        t.nodes
                            .get(captured)
                            .and_then(|n| n.interaction.as_ref())
                            .and_then(|i| i.on_click.clone())
                    });
                    if let Some(handler) = handler {
                        batch(|| handler());
                    }
                }
            }
            InputEvent::PointerLeave => self.set_hover(None),
            _ => {}
        }
    }

    fn draw(&mut self, painter: &mut Painter<'_>, frame: &FrameInfo) {
        // Clamp so a hitch can't fling springs across the screen.
        let dt = frame.dt.as_secs_f64().min(1.0 / 20.0);
        let resized = self.last_size != Some(frame.size);
        self.last_size = Some(frame.size);
        painter.fill_rect(
            rugkit_render::Rect::from_origin_size(rugkit_render::Point::ZERO, frame.size),
            0.0,
            crate::theme::theme().background,
        );
        let tree = self.tree.clone();
        tree.with(|t| {
            if t.needs_layout || resized {
                t.compute_layout(frame.size, painter);
            }
            self.animating = t.step(dt, frame.reduce_motion);
            t.paint(painter);
            t.needs_frame = false;
        });
    }

    fn wants_frame(&self) -> bool {
        self.animating || self.tree.with(|t| t.needs_frame)
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
    use rugkit_shell::{FrameStats, InputEvent};
    use std::time::Duration;

    fn frame(size: Size, dt: f64, reduce_motion: bool) -> FrameInfo {
        FrameInfo {
            size,
            scale: 2.0,
            time: Duration::ZERO,
            dt: Duration::from_secs_f64(dt),
            stats: FrameStats::default(),
            reduce_motion,
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
