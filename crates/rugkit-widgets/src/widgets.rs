//! Built-in widgets: [`text`], [`button`], [`row`] and [`column()`].

use std::rc::Rc;

use rugkit_layout::{Align, Axis, Insets, Justify, LayoutStyle};
use rugkit_reactive::signal;
use rugkit_render::{Color, FontFamily, FontWeight, TextStyle};

use crate::common::{Common, common_modifiers};
use crate::prop::{IntoProp, Prop};
use crate::theme::theme;
use crate::tree::{Content, Interaction, Node, NodeId};
use crate::view::{BuildCx, Children, View};

// ---- Text -------------------------------------------------------------------------------

/// A run of text. Created with [`text`].
pub struct Text {
    content: Prop<String>,
    style: TextStyle,
    color: Option<Prop<Color>>,
    common: Common,
}

impl std::fmt::Debug for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Text").field("style", &self.style).finish()
    }
}

/// Text with static or reactive content.
///
/// ```
/// use rugkit_reactive::signal;
/// use rugkit_widgets::text;
/// use rugkit_render::FontWeight;
///
/// let name = signal(String::from("Ada"));
/// let greeting = text(move || format!("Hi, {}!", name.get())).size(28.0).weight(FontWeight::BOLD);
/// ```
pub fn text(content: impl IntoProp<String>) -> Text {
    let t = theme();
    Text {
        content: content.into_prop(),
        style: TextStyle::new(17.0).color(t.ink),
        color: None,
        common: Common::default(),
    }
}

impl Text {
    /// Font size in logical pixels.
    pub fn size(mut self, size: f32) -> Self {
        self.style.size = size;
        self
    }

    /// Font weight.
    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.style.weight = weight;
        self
    }

    /// Typeface. The default is the rounded UI face.
    pub fn family(mut self, family: FontFamily) -> Self {
        self.style.family = family;
        self
    }

    /// Extra space between letters (negative tightens).
    pub fn letter_spacing(mut self, spacing: f32) -> Self {
        self.style.letter_spacing = spacing;
        self
    }

    /// Text colour (static or reactive). Changes animate.
    pub fn color(mut self, color: impl IntoProp<Color>) -> Self {
        self.color = Some(color.into_prop());
        self
    }
}

common_modifiers!(Text);

impl View for Text {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let initial_color = self.style.color;
        let id = cx.node(
            LayoutStyle::default(),
            Content::Text {
                text: String::new(),
                style: self.style,
            },
        );
        cx.with_tree(|t| t.nodes[id].visual.text_color.snap(initial_color));
        cx.bind(id, self.content, |t, id, v| t.set_text(id, v));
        if let Some(color) = self.color {
            cx.bind(id, color, |t, id, v| {
                let n = &mut t.nodes[id];
                Node::animate(n.laid_out, &mut n.visual.text_color, v);
            });
        }
        self.common.apply(cx, id);
        id
    }
}

// ---- Button -----------------------------------------------------------------------------

#[derive(Copy, Clone, Debug, PartialEq)]
enum Variant {
    Primary,
    Secondary,
}

/// A pressable button. Created with [`button`].
///
/// Hover lifts it and deepens its shadow, pressing squashes it, and every change springs.
pub struct Button {
    label: Prop<String>,
    variant: Variant,
    tint: Option<Color>,
    on_click: Option<Rc<dyn Fn()>>,
    common: Common,
}

impl std::fmt::Debug for Button {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Button")
            .field("variant", &self.variant)
            .finish_non_exhaustive()
    }
}

/// A button with a static or reactive label. Primary (accent-filled) by default.
///
/// ```
/// use rugkit_reactive::signal;
/// use rugkit_widgets::button;
///
/// let count = signal(0);
/// let plus = button("+1").on_click(move || count.update(|c| *c += 1));
/// ```
pub fn button(label: impl IntoProp<String>) -> Button {
    Button {
        label: label.into_prop(),
        variant: Variant::Primary,
        tint: None,
        on_click: None,
        common: Common::default(),
    }
}

impl Button {
    /// Runs `f` when the button is clicked (pressed and released over it).
    pub fn on_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// Accent-filled with a glossy gradient and tinted glow (the default).
    pub fn primary(mut self) -> Self {
        self.variant = Variant::Primary;
        self
    }

    /// White with a soft neutral shadow, for secondary actions.
    pub fn secondary(mut self) -> Self {
        self.variant = Variant::Secondary;
        self
    }

    /// A primary button in `color` instead of the theme accent.
    pub fn tint(mut self, color: Color) -> Self {
        self.variant = Variant::Primary;
        self.tint = Some(color);
        self
    }
}

common_modifiers!(Button);

impl View for Button {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let t = theme();
        let hovered = signal(false);
        let pressed = signal(false);
        let (base, fg, rest_elevation, tint, gloss) = match self.variant {
            Variant::Primary => {
                let c = self.tint.unwrap_or(t.accent);
                (c, Color::WHITE, 6.0, c, true)
            }
            Variant::Secondary => (t.surface, t.ink, 3.0, t.ink, false),
        };

        let id = cx.node(
            LayoutStyle {
                axis: Axis::Row,
                padding: Insets::symmetric(14.0, 26.0),
                align: Align::Center,
                justify: Justify::Center,
                min_width: Some(52.0),
                ..Default::default()
            },
            Content::None,
        );
        cx.with_tree(|tree| {
            let n = &mut tree.nodes[id];
            n.interaction = Some(Interaction {
                hovered,
                pressed,
                on_click: self.on_click,
            });
            n.visual.gloss = gloss;
            n.visual.shadow_tint = tint;
            n.visual.radius.snap(999.0);
            if self.variant == Variant::Secondary {
                n.visual.border_width = 1.0;
                n.visual.border_color.snap(t.ink.with_alpha(0.07));
            }
        });

        let label = text(self.label)
            .size(17.0)
            .weight(FontWeight::BOLD)
            .color(fg)
            .build(cx);
        cx.set_children(id, vec![label]);

        cx.bind(
            id,
            Prop::Dynamic(Rc::new(move || {
                if pressed.get() {
                    base.mix(Color::BLACK, 0.10)
                } else if hovered.get() {
                    base.mix(Color::WHITE, 0.10)
                } else {
                    base
                }
            })),
            |tree, id, v| {
                let n = &mut tree.nodes[id];
                Node::animate(n.laid_out, &mut n.visual.background, v);
            },
        );
        cx.bind(
            id,
            Prop::Dynamic(Rc::new(move || match (hovered.get(), pressed.get()) {
                (_, true) => (rest_elevation * 0.3, 0.94, 1.0),
                (true, false) => (rest_elevation * 1.7, 1.03, -2.0),
                (false, false) => (rest_elevation, 1.0, 0.0),
            })),
            |tree, id, (elevation, scale, lift)| {
                let n = &mut tree.nodes[id];
                Node::animate(n.laid_out, &mut n.visual.elevation, elevation);
                Node::animate(n.laid_out, &mut n.visual.scale, scale);
                Node::animate(n.laid_out, &mut n.visual.lift, lift);
            },
        );
        self.common.apply(cx, id);
        id
    }
}

// ---- Stack (row / column) ---------------------------------------------------------------

type BuildChildren = Box<dyn FnOnce(&mut BuildCx) -> Vec<NodeId>>;

/// A row or column of children. Created with [`row`] or [`column()`].
pub struct Stack {
    style: LayoutStyle,
    children: BuildChildren,
    common: Common,
}

impl std::fmt::Debug for Stack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stack").field("style", &self.style).finish()
    }
}

fn stack(axis: Axis, children: impl Children + 'static) -> Stack {
    Stack {
        style: LayoutStyle {
            axis,
            align: Align::Center,
            ..Default::default()
        },
        children: Box::new(move |cx| children.build_children(cx)),
        common: Common::default(),
    }
}

/// Lays children out left to right, vertically centred.
///
/// ```
/// use rugkit_widgets::{button, row, text};
///
/// let toolbar = row((text("Volume"), button("−"), button("+"))).gap(12.0);
/// ```
pub fn row(children: impl Children + 'static) -> Stack {
    stack(Axis::Row, children)
}

/// Lays children out top to bottom, horizontally centred.
pub fn column(children: impl Children + 'static) -> Stack {
    stack(Axis::Column, children)
}

impl Stack {
    /// Space between children.
    pub fn gap(mut self, gap: f64) -> Self {
        self.style.gap = gap;
        self
    }

    /// Cross-axis alignment of children.
    pub fn align(mut self, align: Align) -> Self {
        self.style.align = align;
        self
    }

    /// Main-axis distribution of children.
    pub fn justify(mut self, justify: Justify) -> Self {
        self.style.justify = justify;
        self
    }
}

common_modifiers!(Stack);

impl View for Stack {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let id = cx.node(self.style, Content::None);
        let children = (self.children)(cx);
        cx.set_children(id, children);
        self.common.apply(cx, id);
        id
    }
}
