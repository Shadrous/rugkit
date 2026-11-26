//! Core M3 controls and containers: [`checkbox`], [`toggle`], [`slider`], [`radio`],
//! [`select`], [`scroll`], [`virtual_list`], [`tabs`], [`modal`], [`popover`] and [`tooltip`].

use std::rc::Rc;

use rugkit_layout::{Align, Axis, Insets, Justify, LayoutStyle, Length};
use rugkit_reactive::{Signal, signal};
use rugkit_render::{Color, FontWeight, Painter, Path, Point, Rect, TextStyle, Vec2};
use rugkit_shell::{Key, Role};

use crate::common::{Common, common_modifiers};
use crate::prop::{IntoProp, Prop};
use crate::tree::{Content, Interaction, Node, NodeId};
use crate::view::{AnyView, BuildCx, View};
use crate::widgets::{button, column, row, text};

// ---- Checkbox ---------------------------------------------------------------------------

/// A checkable box with a spring-animated checkmark. Created with [`checkbox`].
pub struct Checkbox {
    checked: Signal<bool>,
    label: Option<String>,
    disabled: Option<Prop<bool>>,
    common: Common,
}

impl std::fmt::Debug for Checkbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Checkbox")
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

/// Creates a [`Checkbox`] bound to `checked`.
pub fn checkbox(checked: Signal<bool>) -> Checkbox {
    Checkbox {
        checked,
        label: None,
        disabled: None,
        common: Common::default(),
    }
}

impl Checkbox {
    /// Text label displayed beside the checkbox (also used as the accessibility label).
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Disables interaction and dims the checkbox, static or reactive.
    pub fn disabled(mut self, disabled: impl IntoProp<bool>) -> Self {
        self.disabled = Some(disabled.into_prop());
        self
    }
}

common_modifiers!(Checkbox);

impl View for Checkbox {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let get_theme = cx.theme_getter();
        let checked = self.checked;
        let hovered = signal(false);
        let pressed = signal(false);
        let focused = signal(false);

        let box_id = cx.node(
            LayoutStyle {
                width: Length::Px(24.0),
                height: Length::Px(24.0),
                ..Default::default()
            },
            Content::Custom(Rc::new(move |p, rect, _| {
                if checked.get_untracked() {
                    let mut path = Path::new();
                    path.move_to(Point::new(
                        rect.x0 + rect.width() * 0.24,
                        rect.y0 + rect.height() * 0.52,
                    ))
                    .line_to(Point::new(
                        rect.x0 + rect.width() * 0.44,
                        rect.y0 + rect.height() * 0.72,
                    ))
                    .line_to(Point::new(
                        rect.x0 + rect.width() * 0.78,
                        rect.y0 + rect.height() * 0.30,
                    ));
                    p.stroke_path(&path, 2.6, Color::WHITE);
                }
            })),
        );
        cx.with_tree(|t| {
            let n = &mut t.nodes[box_id];
            n.visual.radius.snap(8.0);
            n.visual.border_width = 2.0;
        });

        let box_theme = get_theme.clone();
        cx.bind(
            box_id,
            Prop::Dynamic(Rc::new(move || {
                let t = box_theme();
                let on = checked.get();
                let bg = if on {
                    if pressed.get() {
                        t.accent.mix(Color::BLACK, 0.1)
                    } else {
                        t.accent
                    }
                } else if hovered.get() {
                    t.surface.mix(t.ink, 0.05)
                } else {
                    t.surface
                };
                let border = if on {
                    t.accent
                } else if hovered.get() {
                    t.ink.with_alpha(0.35)
                } else {
                    t.border
                };
                let scale = if pressed.get() {
                    0.90
                } else if hovered.get() {
                    1.05
                } else {
                    1.0
                };
                let ring = if focused.get() { 1.0 } else { 0.0 };
                (bg, border, scale, ring)
            })),
            |t, id, (bg, border, scale, ring)| {
                let n = &mut t.nodes[id];
                Node::animate(n.laid_out, &mut n.visual.background, bg);
                Node::animate(n.laid_out, &mut n.visual.border_color, border);
                Node::animate(n.laid_out, &mut n.visual.scale, scale);
                Node::animate(n.laid_out, &mut n.visual.focus_ring, ring);
            },
        );

        let container_id = cx.node(
            LayoutStyle {
                axis: Axis::Row,
                gap: 10.0,
                align: Align::Center,
                ..Default::default()
            },
            Content::None,
        );
        let mut children = vec![box_id];
        if let Some(label_text) = &self.label {
            let lbl = text(label_text.clone()).size(16.0).build(cx);
            children.push(lbl);
        }
        cx.set_children(container_id, children);
        cx.with_tree(|t| {
            let n = &mut t.nodes[container_id];
            n.role = Some(Role::Checkbox);
            n.a11y_label = self.label;
            n.a11y_checked = Some(Rc::new(move || checked.get_untracked()));
            n.interaction = Some(Interaction {
                hovered,
                pressed,
                focused,
                focusable: true,
                on_click: Some(Rc::new(move || checked.update(|v| *v = !*v))),
                on_drag: None,
                on_pointer: None,
                on_key: None,
                on_scroll: None,
            });
        });
        if let Some(disabled) = self.disabled {
            cx.bind(container_id, disabled, |t, id, d| {
                t.nodes[id].disabled = d;
            });
        }
        self.common.apply(cx, container_id);
        container_id
    }
}

// ---- Toggle -----------------------------------------------------------------------------

/// A bouncy pill switch. Created with [`toggle`].
pub struct Toggle {
    on: Signal<bool>,
    label: Option<String>,
    disabled: Option<Prop<bool>>,
    common: Common,
}

impl std::fmt::Debug for Toggle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Toggle")
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

/// Creates a [`Toggle`] switch bound to `on`.
pub fn toggle(on: Signal<bool>) -> Toggle {
    Toggle {
        on,
        label: None,
        disabled: None,
        common: Common::default(),
    }
}

impl Toggle {
    /// Text label shown beside the switch.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Disables interaction and dims the switch, static or reactive.
    pub fn disabled(mut self, disabled: impl IntoProp<bool>) -> Self {
        self.disabled = Some(disabled.into_prop());
        self
    }
}

common_modifiers!(Toggle);

impl View for Toggle {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let get_theme = cx.theme_getter();
        let on = self.on;
        let hovered = signal(false);
        let pressed = signal(false);
        let focused = signal(false);

        let track_id = cx.node(
            LayoutStyle {
                axis: Axis::Row,
                width: Length::Px(52.0),
                height: Length::Px(30.0),
                padding: Insets::all(3.0),
                align: Align::Center,
                justify: Justify::Start,
                ..Default::default()
            },
            Content::None,
        );
        let thumb_id = cx.node(
            LayoutStyle {
                width: Length::Px(24.0),
                height: Length::Px(24.0),
                ..Default::default()
            },
            Content::None,
        );
        cx.set_children(track_id, vec![thumb_id]);
        cx.with_tree(|t| {
            t.nodes[track_id].visual.radius.snap(999.0);
            t.nodes[thumb_id].visual.radius.snap(999.0);
            t.nodes[thumb_id].visual.background.snap(Color::WHITE);
            t.nodes[thumb_id].visual.elevation.snap(3.0);
        });

        // Spring the thumb horizontally via track justify_content (Start vs End)!
        cx.bind(
            track_id,
            Prop::Dynamic(Rc::new(move || {
                let t = get_theme();
                let active = on.get();
                let bg = if active {
                    t.accent
                } else if hovered.get() {
                    t.ink.with_alpha(0.24)
                } else {
                    t.ink.with_alpha(0.16)
                };
                let ring = if focused.get() { 1.0 } else { 0.0 };
                (bg, active, ring)
            })),
            |t, id, (bg, active, ring)| {
                t.update_style(id, |s| {
                    s.justify = if active { Justify::End } else { Justify::Start };
                });
                let n = &mut t.nodes[id];
                Node::animate(n.laid_out, &mut n.visual.background, bg);
                Node::animate(n.laid_out, &mut n.visual.focus_ring, ring);
            },
        );
        cx.bind(
            thumb_id,
            Prop::Dynamic(Rc::new(move || if pressed.get() { 28.0 } else { 24.0 })),
            |t, id, w| {
                t.update_style(id, |s| s.width = Length::Px(w));
            },
        );

        let container_id = cx.node(
            LayoutStyle {
                axis: Axis::Row,
                gap: 10.0,
                align: Align::Center,
                ..Default::default()
            },
            Content::None,
        );
        let mut children = vec![track_id];
        if let Some(label_text) = &self.label {
            children.push(text(label_text.clone()).size(16.0).build(cx));
        }
        cx.set_children(container_id, children);
        cx.with_tree(|t| {
            let n = &mut t.nodes[container_id];
            n.role = Some(Role::Switch);
            n.a11y_label = self.label;
            n.a11y_checked = Some(Rc::new(move || on.get_untracked()));
            n.interaction = Some(Interaction {
                hovered,
                pressed,
                focused,
                focusable: true,
                on_click: Some(Rc::new(move || on.update(|v| *v = !*v))),
                on_drag: None,
                on_pointer: None,
                on_key: None,
                on_scroll: None,
            });
        });
        if let Some(disabled) = self.disabled {
            cx.bind(container_id, disabled, |t, id, d| {
                t.nodes[id].disabled = d;
            });
        }
        self.common.apply(cx, container_id);
        container_id
    }
}

// ---- Slider -----------------------------------------------------------------------------

/// A draggable and keyboard-adjustable range slider. Created with [`slider`].
pub struct Slider {
    value: Signal<f64>,
    min: f64,
    max: f64,
    step: Option<f64>,
    disabled: Option<Prop<bool>>,
    common: Common,
}

impl std::fmt::Debug for Slider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Slider")
            .field("min", &self.min)
            .field("max", &self.max)
            .finish_non_exhaustive()
    }
}

/// Creates a horizontal [`Slider`] bound to `value` in `min..=max`.
pub fn slider(value: Signal<f64>, min: f64, max: f64) -> Slider {
    Slider {
        value,
        min,
        max,
        step: None,
        disabled: None,
        common: Common::default(),
    }
}

impl Slider {
    /// Snaps slider values to increments of `step`.
    pub fn step(mut self, step: f64) -> Self {
        self.step = Some(step.abs());
        self
    }

    /// Disables interaction and dims the slider, static or reactive.
    pub fn disabled(mut self, disabled: impl IntoProp<bool>) -> Self {
        self.disabled = Some(disabled.into_prop());
        self
    }
}

common_modifiers!(Slider);

impl View for Slider {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let get_theme = cx.theme_getter();
        let value = self.value;
        let min = self.min;
        let max = self.max.max(min + f64::EPSILON);
        let step = self.step;
        let hovered = signal(false);
        let pressed = signal(false);
        let focused = signal(false);

        let quantise = move |raw: f64| -> f64 {
            let clamped = raw.clamp(min, max);
            if let Some(st) = step
                && st > 0.0
            {
                let snapped = min + ((clamped - min) / st).round() * st;
                snapped.clamp(min, max)
            } else {
                clamped
            }
        };

        let draw_theme = get_theme.clone();
        let id = cx.node(
            LayoutStyle {
                width: Length::Px(220.0),
                height: Length::Px(32.0),
                ..Default::default()
            },
            Content::Custom(Rc::new(move |p, rect, _| {
                let t = draw_theme();
                let frac = ((value.get_untracked() - min) / (max - min)).clamp(0.0, 1.0);
                let pad = 12.0;
                let track_w = (rect.width() - pad * 2.0).max(1.0);
                let track_y = rect.center().y - 4.0;
                let track = Rect::from_xywh(rect.x0 + pad, track_y, track_w, 8.0);
                p.fill_rect(track, 4.0, t.ink.with_alpha(0.12));

                let fill_w = (track_w * frac).max(8.0);
                p.fill_rect(
                    Rect::from_xywh(track.x0, track_y, fill_w, 8.0),
                    4.0,
                    t.accent,
                );

                let thumb_x = track.x0 + track_w * frac;
                let thumb_r = if pressed.get_untracked() {
                    12.5
                } else if hovered.get_untracked() {
                    11.5
                } else {
                    10.5
                };
                p.fill_circle(
                    Point::new(thumb_x, rect.center().y + 1.5),
                    thumb_r,
                    t.ink.with_alpha(0.16),
                );
                p.fill_circle(Point::new(thumb_x, rect.center().y), thumb_r, Color::WHITE);
                p.stroke_rect(
                    Rect::from_xywh(
                        thumb_x - thumb_r,
                        rect.center().y - thumb_r,
                        thumb_r * 2.0,
                        thumb_r * 2.0,
                    ),
                    thumb_r,
                    2.0,
                    t.accent,
                );
            })),
        );

        let key_step = step.unwrap_or((max - min) / 20.0);
        cx.with_tree(|t| {
            let n = &mut t.nodes[id];
            n.role = Some(Role::Slider);
            n.visual.radius.snap(16.0);
            n.a11y_numeric = Some(Rc::new(move || (value.get_untracked(), min, max)));
            n.interaction = Some(Interaction {
                hovered,
                pressed,
                focused,
                focusable: true,
                on_click: None,
                on_drag: Some(Rc::new(move |pt, rect| {
                    let pad = 12.0;
                    let track_w = (rect.width() - pad * 2.0).max(1.0);
                    let frac = ((pt.x - (rect.x0 + pad)) / track_w).clamp(0.0, 1.0);
                    value.set_if_changed(quantise(min + frac * (max - min)));
                })),
                on_pointer: None,
                on_key: Some(Rc::new(move |key, _mods| match key {
                    Key::ArrowLeft | Key::ArrowDown => {
                        value.set_if_changed(quantise(value.get_untracked() - key_step));
                        true
                    }
                    Key::ArrowRight | Key::ArrowUp => {
                        value.set_if_changed(quantise(value.get_untracked() + key_step));
                        true
                    }
                    Key::Home => {
                        value.set_if_changed(min);
                        true
                    }
                    Key::End => {
                        value.set_if_changed(max);
                        true
                    }
                    _ => false,
                })),
                on_scroll: None,
            });
        });

        if let Some(disabled) = self.disabled {
            cx.bind(id, disabled, |t, id, d| {
                t.nodes[id].disabled = d;
            });
        }
        cx.bind(
            id,
            Prop::Dynamic(Rc::new(move || {
                let _ = value.get();
                let _ = hovered.get();
                let _ = pressed.get();
                let _ = get_theme();
                if focused.get() { 1.0 } else { 0.0 }
            })),
            |t, id, ring| {
                let n = &mut t.nodes[id];
                Node::animate(n.laid_out, &mut n.visual.focus_ring, ring);
            },
        );
        self.common.apply(cx, id);
        id
    }
}

// ---- Radio ------------------------------------------------------------------------------

/// A single option in a radio group. Created with [`radio`].
pub struct Radio<T: PartialEq + Clone + 'static> {
    selected: Signal<T>,
    value: T,
    label: Option<String>,
    disabled: Option<Prop<bool>>,
    common: Common,
}

impl<T: PartialEq + Clone + 'static> std::fmt::Debug for Radio<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Radio")
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

/// Creates a [`Radio`] button that selects `value` in `selected`.
pub fn radio<T: PartialEq + Clone + 'static>(selected: Signal<T>, value: T) -> Radio<T> {
    Radio {
        selected,
        value,
        label: None,
        disabled: None,
        common: Common::default(),
    }
}

impl<T: PartialEq + Clone + 'static> Radio<T> {
    /// Text label displayed beside the radio button.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Disables interaction and dims the radio button, static or reactive.
    pub fn disabled(mut self, disabled: impl IntoProp<bool>) -> Self {
        self.disabled = Some(disabled.into_prop());
        self
    }
}

common_modifiers!(Radio<T> where T: PartialEq + Clone + 'static);

impl<T: PartialEq + Clone + 'static> View for Radio<T> {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let get_theme = cx.theme_getter();
        let selected = self.selected;
        let val_draw = self.value.clone();
        let val_bind = self.value.clone();
        let val_click = self.value.clone();
        let val_a11y = self.value;
        let hovered = signal(false);
        let pressed = signal(false);
        let focused = signal(false);

        let circle_id = cx.node(
            LayoutStyle {
                width: Length::Px(22.0),
                height: Length::Px(22.0),
                ..Default::default()
            },
            Content::Custom(Rc::new(move |p, rect, color| {
                if selected.with_untracked(|s| *s == val_draw) {
                    p.fill_circle(rect.center(), 5.5, color);
                }
            })),
        );
        cx.with_tree(|t| {
            let n = &mut t.nodes[circle_id];
            n.visual.radius.snap(999.0);
            n.visual.border_width = 2.0;
        });

        cx.bind(
            circle_id,
            Prop::Dynamic(Rc::new(move || {
                let t = get_theme();
                let is_on = selected.with(|s| *s == val_bind);
                let border = if is_on {
                    t.accent
                } else if hovered.get() {
                    t.ink.with_alpha(0.38)
                } else {
                    t.border
                };
                let scale = if pressed.get() { 0.90 } else { 1.0 };
                let ring = if focused.get() { 1.0 } else { 0.0 };
                (t.surface, border, t.accent, scale, ring)
            })),
            |t, id, (bg, border, dot, scale, ring)| {
                let n = &mut t.nodes[id];
                Node::animate(n.laid_out, &mut n.visual.background, bg);
                Node::animate(n.laid_out, &mut n.visual.border_color, border);
                Node::animate(n.laid_out, &mut n.visual.text_color, dot);
                Node::animate(n.laid_out, &mut n.visual.scale, scale);
                Node::animate(n.laid_out, &mut n.visual.focus_ring, ring);
            },
        );

        let container_id = cx.node(
            LayoutStyle {
                axis: Axis::Row,
                gap: 10.0,
                align: Align::Center,
                ..Default::default()
            },
            Content::None,
        );
        let mut children = vec![circle_id];
        if let Some(label_text) = &self.label {
            children.push(text(label_text.clone()).size(16.0).build(cx));
        }
        cx.set_children(container_id, children);
        cx.with_tree(|t| {
            let n = &mut t.nodes[container_id];
            n.role = Some(Role::RadioButton);
            n.a11y_label = self.label;
            n.a11y_checked = Some(Rc::new(move || selected.with_untracked(|s| *s == val_a11y)));
            n.interaction = Some(Interaction {
                hovered,
                pressed,
                focused,
                focusable: true,
                on_click: Some(Rc::new(move || selected.set(val_click.clone()))),
                on_drag: None,
                on_pointer: None,
                on_key: None,
                on_scroll: None,
            });
        });
        if let Some(disabled) = self.disabled {
            cx.bind(container_id, disabled, |t, id, d| {
                t.nodes[id].disabled = d;
            });
        }
        self.common.apply(cx, container_id);
        container_id
    }
}

// ---- Select / Dropdown ------------------------------------------------------------------

/// A dropdown select control with keyboard and pointer selection. Created with [`select`].
pub struct Select {
    selected: Signal<usize>,
    options: Vec<String>,
    disabled: Option<Prop<bool>>,
    common: Common,
}

impl std::fmt::Debug for Select {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Select")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

/// Creates a [`Select`] dropdown choosing an index into `options`.
pub fn select(selected: Signal<usize>, options: Vec<String>) -> Select {
    Select {
        selected,
        options,
        disabled: None,
        common: Common::default(),
    }
}

impl Select {
    /// Disables interaction and dims the dropdown, static or reactive.
    pub fn disabled(mut self, disabled: impl IntoProp<bool>) -> Self {
        self.disabled = Some(disabled.into_prop());
        self
    }
}

common_modifiers!(Select);

impl View for Select {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let get_theme = cx.theme_getter();
        let selected = self.selected;
        let open = signal(false);
        let options = Rc::new(self.options);
        let opts_label = options.clone();

        let trigger_id = button(move || {
            let idx = selected.get();
            let current = opts_label.get(idx).map_or("Select…", String::as_str);
            format!("{current}  ▾")
        })
        .secondary()
        .radius(14.0)
        .on_click(move || open.update(|o| *o = !*o))
        .build(cx);

        cx.with_tree(|t| {
            t.nodes[trigger_id].role = Some(Role::ComboBox);
        });
        if let Some(disabled) = self.disabled {
            cx.bind(trigger_id, disabled, |t, id, d| {
                t.nodes[id].disabled = d;
            });
        }

        let mut option_views: Vec<AnyView> = Vec::new();
        for (idx, opt) in options.iter().enumerate() {
            let opt_label = opt.clone();
            let opt_theme = get_theme.clone();
            let item = button(opt_label)
                .secondary()
                .radius(10.0)
                .padding((8.0, 14.0))
                .fill_width()
                .background(move || {
                    let t = opt_theme();
                    if selected.get() == idx {
                        t.accent.with_alpha(0.16)
                    } else {
                        Color::TRANSPARENT
                    }
                })
                .elevation(0.0)
                .on_click(move || {
                    selected.set(idx);
                    open.set(false);
                })
                .into_any();
            option_views.push(item);
        }

        let menu_theme = get_theme.clone();
        let menu_border = get_theme;
        let menu_id = column(option_views)
            .gap(4.0)
            .padding(6.0)
            .background(move || menu_theme().surface_raised)
            .border(1.0, move || menu_border().border)
            .radius(16.0)
            .elevation(10.0)
            .visible(move || open.get())
            .build(cx);

        let root_id = cx.node(
            LayoutStyle {
                axis: Axis::Column,
                gap: 6.0,
                align: Align::Stretch,
                ..Default::default()
            },
            Content::None,
        );
        cx.set_children(root_id, vec![trigger_id, menu_id]);
        self.common.apply(cx, root_id);
        root_id
    }
}

// ---- Scroll -----------------------------------------------------------------------------

/// A vertically scrollable clipped viewport with smooth spring scrolling. Created with [`scroll`].
pub struct Scroll<V> {
    child: V,
    common: Common,
}

impl<V> std::fmt::Debug for Scroll<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scroll").finish_non_exhaustive()
    }
}

/// Wraps `child` in a scrollable viewport (give it a `.height(...)` to constrain the viewport).
pub fn scroll<V: View>(child: V) -> Scroll<V> {
    Scroll {
        child,
        common: Common::default(),
    }
}

common_modifiers!(Scroll<V> where V: View);

impl<V: View> View for Scroll<V> {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let id = cx.node(
            LayoutStyle {
                axis: Axis::Column,
                align: Align::Stretch,
                ..Default::default()
            },
            Content::None,
        );
        let child_id = self.child.build(cx);
        cx.set_children(id, vec![child_id]);
        cx.with_tree(|t| {
            let n = &mut t.nodes[id];
            n.role = Some(Role::ScrollView);
            n.clip_content = true;
        });
        self.common.apply(cx, id);
        id
    }
}

// ---- Virtualised List -------------------------------------------------------------------

type RowRenderer = Rc<dyn Fn(&mut Painter<'_>, usize, Rect)>;

/// A virtualised list that renders only visible rows (smooth at 100k+ items).
/// Created with [`virtual_list`].
pub struct VirtualList {
    count: Prop<usize>,
    item_height: f64,
    render_row: RowRenderer,
    common: Common,
}

impl std::fmt::Debug for VirtualList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VirtualList")
            .field("item_height", &self.item_height)
            .finish_non_exhaustive()
    }
}

/// Creates a virtualised list of `count` fixed-height rows (`item_height` logical pixels).
///
/// Only rows intersecting the visible viewport are passed to `render_row(painter, index, rect)`.
pub fn virtual_list(
    count: impl IntoProp<usize>,
    item_height: f64,
    render_row: impl Fn(&mut Painter<'_>, usize, Rect) + 'static,
) -> VirtualList {
    VirtualList {
        count: count.into_prop(),
        item_height: item_height.max(1.0),
        render_row: Rc::new(render_row),
        common: Common::default(),
    }
}

common_modifiers!(VirtualList);

impl View for VirtualList {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let get_theme = cx.theme_getter();
        let count_sig = signal(0usize);
        let scroll_offset = signal(0.0f64);
        let item_h = self.item_height;
        let render_row = self.render_row;

        let id = cx.node(
            LayoutStyle {
                width: Length::Px(320.0),
                height: Length::Px(220.0),
                ..Default::default()
            },
            Content::Custom(Rc::new(move |p, rect, _| {
                let total = count_sig.get_untracked();
                let total_h = (total as f64) * item_h;
                let max_scroll = (total_h - rect.height()).max(0.0);
                let y_off = scroll_offset.get_untracked().clamp(0.0, max_scroll);
                let first = (y_off / item_h).floor() as usize;
                let visible = (rect.height() / item_h).ceil() as usize + 1;
                let last = (first + visible).min(total);

                p.with_clip(rect, 12.0, |p| {
                    for i in first..last {
                        let row_y = rect.y0 + (i as f64) * item_h - y_off;
                        let row_rect = Rect::from_xywh(rect.x0, row_y, rect.width(), item_h);
                        render_row(p, i, row_rect);
                    }
                    if max_scroll > 1.0 && rect.height() > 16.0 {
                        let thumb_h = ((rect.height() / total_h) * rect.height())
                            .clamp(24.0, rect.height() - 8.0);
                        let frac = (y_off / max_scroll).clamp(0.0, 1.0);
                        let thumb_y = rect.y0 + 4.0 + frac * (rect.height() - thumb_h - 8.0);
                        p.fill_rect(
                            Rect::from_xywh(rect.x1 - 8.0, thumb_y, 4.0, thumb_h),
                            2.0,
                            get_theme().ink.with_alpha(0.24),
                        );
                    }
                });
            })),
        );

        let hovered = signal(false);
        let pressed = signal(false);
        let focused = signal(false);
        cx.bind(id, self.count, move |_, _, n| count_sig.set(n));
        cx.bind(
            id,
            Prop::Dynamic(Rc::new(move || scroll_offset.get())),
            |_, _, _| {},
        );
        cx.with_tree(|t| {
            let n = &mut t.nodes[id];
            n.role = Some(Role::List);
            n.visual.radius.snap(14.0);
            n.interaction = Some(Interaction {
                hovered,
                pressed,
                focused,
                focusable: true,
                on_click: None,
                on_drag: None,
                on_pointer: None,
                on_key: Some(Rc::new(move |key, _| match key {
                    Key::ArrowDown => {
                        scroll_offset.update(|y| *y = (*y + item_h).max(0.0));
                        true
                    }
                    Key::ArrowUp => {
                        scroll_offset.update(|y| *y = (*y - item_h).max(0.0));
                        true
                    }
                    _ => false,
                })),
                on_scroll: Some(Rc::new(move |delta: Vec2| {
                    let max = ((count_sig.get_untracked() as f64) * item_h).max(0.0);
                    scroll_offset.update(|y| *y = (*y - delta.y).clamp(0.0, max));
                })),
            });
        });
        self.common.apply(cx, id);
        id
    }
}

// ---- Tabs -------------------------------------------------------------------------------

/// A tabbed panel with a pill tab bar. Created with [`tabs`].
pub struct Tabs {
    active: Signal<usize>,
    items: Vec<(String, AnyView)>,
    common: Common,
}

impl std::fmt::Debug for Tabs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tabs")
            .field("count", &self.items.len())
            .finish_non_exhaustive()
    }
}

/// Creates a [`Tabs`] widget switching between `(label, view)` pairs controlled by `active`.
pub fn tabs(active: Signal<usize>, items: Vec<(String, AnyView)>) -> Tabs {
    Tabs {
        active,
        items,
        common: Common::default(),
    }
}

common_modifiers!(Tabs);

impl View for Tabs {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let get_theme = cx.theme_getter();
        let active = self.active;
        let mut tab_buttons: Vec<AnyView> = Vec::new();
        let mut panel_ids: Vec<NodeId> = Vec::new();

        for (idx, (label, panel)) in self.items.into_iter().enumerate() {
            let t_bg = get_theme.clone();
            let t_fg = get_theme.clone();
            let btn = button(label)
                .secondary()
                .padding((8.0, 18.0))
                .radius(999.0)
                .label_color(move || {
                    if active.get() == idx {
                        Color::WHITE
                    } else {
                        t_fg().ink
                    }
                })
                .background(move || {
                    let t = t_bg();
                    if active.get() == idx {
                        t.accent
                    } else {
                        t.surface
                    }
                })
                .elevation(move || if active.get() == idx { 4.0 } else { 0.0 })
                .on_click(move || active.set(idx))
                .into_any();
            tab_buttons.push(btn);

            let panel_wrap = cx.node(
                LayoutStyle {
                    axis: Axis::Column,
                    align: Align::Stretch,
                    ..Default::default()
                },
                Content::None,
            );
            let p_child = panel.build(cx);
            cx.set_children(panel_wrap, vec![p_child]);
            cx.bind(
                panel_wrap,
                Prop::Dynamic(Rc::new(move || active.get() == idx)),
                |t, id, show| {
                    t.update_style(id, |s| s.hidden = !show);
                },
            );
            panel_ids.push(panel_wrap);
        }

        let bar_bg = get_theme;
        let bar_id = row(tab_buttons)
            .gap(6.0)
            .padding(5.0)
            .radius(999.0)
            .background(move || bar_bg().ink.with_alpha(0.06))
            .build(cx);
        cx.with_tree(|t| {
            t.nodes[bar_id].role = Some(Role::TabList);
            let tab_children = t.nodes[bar_id].children.clone();
            for (idx, cid) in tab_children.into_iter().enumerate() {
                t.nodes[cid].role = Some(Role::Tab);
                t.nodes[cid].a11y_checked = Some(Rc::new(move || active.get_untracked() == idx));
            }
        });

        let root_id = cx.node(
            LayoutStyle {
                axis: Axis::Column,
                gap: 14.0,
                align: Align::Stretch,
                ..Default::default()
            },
            Content::None,
        );
        let mut all = Vec::with_capacity(1 + panel_ids.len());
        all.push(bar_id);
        all.extend(panel_ids);
        cx.set_children(root_id, all);
        self.common.apply(cx, root_id);
        root_id
    }
}

// ---- Modal ------------------------------------------------------------------------------

/// A centred modal dialog with a dimmed backdrop. Created with [`modal`].
pub struct Modal<V> {
    open: Signal<bool>,
    content: V,
    common: Common,
}

impl<V> std::fmt::Debug for Modal<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Modal").finish_non_exhaustive()
    }
}

/// Creates a [`Modal`] overlay shown when `open` is `true` (place inside a `zstack` over the page).
pub fn modal<V: View>(open: Signal<bool>, content: V) -> Modal<V> {
    Modal {
        open,
        content,
        common: Common::default(),
    }
}

common_modifiers!(Modal<V> where V: View);

impl<V: View> View for Modal<V> {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let get_theme = cx.theme_getter();
        let open = self.open;
        let hovered = signal(false);
        let pressed = signal(false);
        let focused = signal(false);

        let card_theme = get_theme.clone();
        let card_border = get_theme;
        let card_id = column((self.content,))
            .padding(28.0)
            .radius(28.0)
            .background(move || card_theme().surface_raised)
            .border(1.0, move || card_border().border)
            .elevation(14.0)
            .scale(move || if open.get() { 1.0 } else { 0.92 })
            .build(cx);
        cx.with_tree(|t| {
            t.nodes[card_id].role = Some(Role::Dialog);
        });

        let backdrop_id = cx.node(
            LayoutStyle {
                axis: Axis::Column,
                align: Align::Center,
                justify: Justify::Center,
                absolute_fill: true,
                ..Default::default()
            },
            Content::None,
        );
        cx.set_children(backdrop_id, vec![card_id]);
        cx.with_tree(|t| {
            let n = &mut t.nodes[backdrop_id];
            n.visual.background.snap(Color::rgba(0.0, 0.0, 0.0, 0.45));
            n.interaction = Some(Interaction {
                hovered,
                pressed,
                focused,
                focusable: false,
                on_click: Some(Rc::new(move || open.set(false))),
                on_drag: None,
                on_pointer: None,
                on_key: Some(Rc::new(move |key, _| {
                    if *key == Key::Escape {
                        open.set(false);
                        true
                    } else {
                        false
                    }
                })),
                on_scroll: None,
            });
        });
        cx.bind(
            backdrop_id,
            Prop::Dynamic(Rc::new(move || open.get())),
            |t, id, show| {
                t.update_style(id, |s| s.hidden = !show);
            },
        );
        self.common.apply(cx, backdrop_id);
        backdrop_id
    }
}

// ---- Popover ----------------------------------------------------------------------------

/// A floating card anchored below an anchor view. Created with [`popover`].
pub struct Popover<A, C> {
    open: Signal<bool>,
    anchor: A,
    content: C,
    common: Common,
}

impl<A, C> std::fmt::Debug for Popover<A, C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Popover").finish_non_exhaustive()
    }
}

/// Shows `content` in a floating surface below `anchor` whenever `open` is `true`.
pub fn popover<A: View, C: View>(open: Signal<bool>, anchor: A, content: C) -> Popover<A, C> {
    Popover {
        open,
        anchor,
        content,
        common: Common::default(),
    }
}

common_modifiers!(Popover<A, C> where A: View, C: View);

impl<A: View, C: View> View for Popover<A, C> {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let get_theme = cx.theme_getter();
        let open = self.open;
        let anchor_id = self.anchor.build(cx);
        let bg_theme = get_theme.clone();
        let border_theme = get_theme;
        let card_id = column((self.content,))
            .padding(14.0)
            .radius(18.0)
            .background(move || bg_theme().surface_raised)
            .border(1.0, move || border_theme().border)
            .elevation(10.0)
            .visible(move || open.get())
            .build(cx);

        let root_id = cx.node(
            LayoutStyle {
                axis: Axis::Column,
                gap: 8.0,
                align: Align::Center,
                ..Default::default()
            },
            Content::None,
        );
        cx.set_children(root_id, vec![anchor_id, card_id]);
        self.common.apply(cx, root_id);
        root_id
    }
}

// ---- Tooltip ----------------------------------------------------------------------------

/// Wraps a child view with a hover tooltip pill. Created with [`tooltip`].
pub struct Tooltip<V> {
    tip: String,
    child: V,
    common: Common,
}

impl<V> std::fmt::Debug for Tooltip<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tooltip").field("tip", &self.tip).finish()
    }
}

/// Attaches a hover tooltip pill with text `tip` to `child`.
pub fn tooltip<V: View>(tip: impl Into<String>, child: V) -> Tooltip<V> {
    Tooltip {
        tip: tip.into(),
        child,
        common: Common::default(),
    }
}

common_modifiers!(Tooltip<V> where V: View);

impl<V: View> View for Tooltip<V> {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let id = self.child.build(cx);
        cx.with_tree(|t| {
            t.nodes[id].tooltip = Some(self.tip);
        });
        self.common.apply(cx, id);
        id
    }
}

/// Paints a floating tooltip pill above `anchor` (called by `Ui::draw` when hovered).
pub(crate) fn paint_tooltip(p: &mut rugkit_render::Painter<'_>, tip: &str, anchor: Rect) {
    let t = crate::theme::theme();
    let style = TextStyle::new(13.0)
        .weight(FontWeight::MEDIUM)
        .color(t.surface);
    let size = p.measure_text(tip, Some(260.0), &style);
    let pad_x = 10.0;
    let pad_y = 6.0;
    let w = size.width + pad_x * 2.0;
    let h = size.height + pad_y * 2.0;
    let x = (anchor.center().x - w * 0.5).max(8.0);
    let y = (anchor.y0 - h - 8.0).max(8.0);
    let rect = Rect::from_xywh(x, y, w, h);
    p.fill_rect(rect, 10.0, t.ink.with_alpha(0.92));
    p.text_wrapped(tip, Point::new(x + pad_x, y + pad_y), 260.0, &style);
}
