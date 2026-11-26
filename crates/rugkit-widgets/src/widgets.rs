//! Core built-in widgets: [`text`], [`button`], [`text_input`], [`row`], [`column()`],
//! [`zstack`], [`grid`] and [`image`].

use std::rc::Rc;

use rugkit_layout::{Align, Axis, Direction, Insets, Justify, LayoutStyle, Length};
use rugkit_reactive::{Signal, signal};
use rugkit_render::{Color, FontFamily, FontWeight, ImageData, Rect, TextStyle};
use rugkit_shell::Role;

use crate::common::{Common, common_modifiers};
use crate::prop::{IntoProp, Prop};
use crate::theme::theme;
use crate::tree::{Content, Interaction, Node, NodeId, TextInputState};
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
        let get_theme = cx.theme_getter();
        let id = cx.node(
            LayoutStyle::default(),
            Content::Text {
                text: String::new(),
                style: self.style,
            },
        );
        cx.with_tree(|t| {
            t.nodes[id].role = Some(Role::Label);
        });
        cx.bind(id, self.content, |t, id, v| t.set_text(id, v));
        let color_prop = self
            .color
            .unwrap_or_else(|| Prop::Dynamic(Rc::new(move || get_theme().ink)));
        cx.bind(id, color_prop, |t, id, v| {
            let n = &mut t.nodes[id];
            Node::animate(n.laid_out, &mut n.visual.text_color, v);
        });
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
/// Hover lifts it and deepens its shadow, pressing squashes it, focusing shows an accent ring,
/// and every change springs.
pub struct Button {
    label: Prop<String>,
    variant: Variant,
    tint: Option<Color>,
    label_color: Option<Prop<Color>>,
    disabled: Option<Prop<bool>>,
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
        label_color: None,
        disabled: None,
        on_click: None,
        common: Common::default(),
    }
}

impl Button {
    /// Runs `f` when the button is clicked or activated via keyboard/VoiceOver.
    pub fn on_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// Accent-filled with a glossy gradient and tinted glow (the default).
    pub fn primary(mut self) -> Self {
        self.variant = Variant::Primary;
        self
    }

    /// Surface-filled with a soft neutral shadow, for secondary actions.
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

    /// Disables the button (dims it and blocks clicks and focus), static or reactive.
    pub fn disabled(mut self, disabled: impl IntoProp<bool>) -> Self {
        self.disabled = Some(disabled.into_prop());
        self
    }

    /// Overrides the label text colour (static or reactive).
    pub fn label_color(mut self, color: impl IntoProp<Color>) -> Self {
        self.label_color = Some(color.into_prop());
        self
    }
}

common_modifiers!(Button);

impl View for Button {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let get_theme = cx.theme_getter();
        let hovered = signal(false);
        let pressed = signal(false);
        let focused = signal(false);
        let variant = self.variant;
        let custom_tint = self.tint;
        let (rest_elevation, gloss) = match variant {
            Variant::Primary => (6.0, true),
            Variant::Secondary => (3.0, false),
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
            n.role = Some(Role::Button);
            n.interaction = Some(Interaction {
                hovered,
                pressed,
                focused,
                focusable: true,
                on_click: self.on_click,
                on_drag: None,
                on_key: None,
                on_scroll: None,
            });
            n.visual.gloss = gloss;
            n.visual.radius.snap(999.0);
            if variant == Variant::Secondary {
                n.visual.border_width = 1.0;
            }
        });

        let label_theme = get_theme.clone();
        let label_color = self.label_color.unwrap_or_else(|| {
            Prop::Dynamic(Rc::new(move || match variant {
                Variant::Primary => Color::WHITE,
                Variant::Secondary => label_theme().ink,
            }))
        });
        let label = text(self.label)
            .size(17.0)
            .weight(FontWeight::BOLD)
            .color(label_color)
            .build(cx);
        cx.set_children(id, vec![label]);

        if let Some(disabled) = self.disabled {
            cx.bind(id, disabled, |tree, id, d| {
                tree.nodes[id].disabled = d;
            });
        }

        let bg_theme = get_theme;
        cx.bind(
            id,
            Prop::Dynamic(Rc::new(move || {
                let t = bg_theme();
                let base = match variant {
                    Variant::Primary => custom_tint.unwrap_or(t.accent),
                    Variant::Secondary => t.surface,
                };
                let bg = if pressed.get() {
                    base.mix(Color::BLACK, 0.10)
                } else if hovered.get() {
                    base.mix(Color::WHITE, 0.10)
                } else {
                    base
                };
                let tint = match variant {
                    Variant::Primary => custom_tint.unwrap_or(t.accent),
                    Variant::Secondary => t.ink,
                };
                let border = if variant == Variant::Secondary {
                    t.border
                } else {
                    Color::TRANSPARENT
                };
                (bg, tint, border)
            })),
            |tree, id, (bg, tint, border)| {
                let n = &mut tree.nodes[id];
                n.visual.shadow_tint = tint;
                Node::animate(n.laid_out, &mut n.visual.background, bg);
                Node::animate(n.laid_out, &mut n.visual.border_color, border);
            },
        );
        cx.bind(
            id,
            Prop::Dynamic(Rc::new(move || {
                let (elev, scale, lift) = match (hovered.get(), pressed.get()) {
                    (_, true) => (rest_elevation * 0.3, 0.94, 1.0),
                    (true, false) => (rest_elevation * 1.7, 1.03, -2.0),
                    (false, false) => (rest_elevation, 1.0, 0.0),
                };
                let ring = if focused.get() { 1.0 } else { 0.0 };
                (elev, scale, lift, ring)
            })),
            |tree, id, (elevation, scale, lift, ring)| {
                let n = &mut tree.nodes[id];
                Node::animate(n.laid_out, &mut n.visual.elevation, elevation);
                Node::animate(n.laid_out, &mut n.visual.scale, scale);
                Node::animate(n.laid_out, &mut n.visual.lift, lift);
                Node::animate(n.laid_out, &mut n.visual.focus_ring, ring);
            },
        );
        self.common.apply(cx, id);
        id
    }
}

// ---- TextInput --------------------------------------------------------------------------

/// A plain text field with full keyboard editing, clipboard and IME (CJK, dead keys, emoji)
/// support. Created with [`text_input`].
pub struct TextInput {
    value: Signal<String>,
    placeholder: String,
    style: TextStyle,
    multiline: bool,
    disabled: Option<Prop<bool>>,
    on_submit: Option<Rc<dyn Fn(String)>>,
    common: Common,
}

impl std::fmt::Debug for TextInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextInput")
            .field("placeholder", &self.placeholder)
            .field("multiline", &self.multiline)
            .finish_non_exhaustive()
    }
}

/// A text input bound to `value`.
///
/// Uses the regular UI typeface (`SF Pro` on macOS) for crisp, dense text editing, and
/// supports CJK IME composition, selection, clipboard shortcuts and VoiceOver.
///
/// ```
/// use rugkit_reactive::signal;
/// use rugkit_widgets::text_input;
///
/// let note = signal(String::new());
/// let field = text_input(note).placeholder("Add a note…");
/// ```
pub fn text_input(value: Signal<String>) -> TextInput {
    TextInput {
        value,
        placeholder: String::new(),
        style: TextStyle::new(16.0).family(FontFamily::Ui),
        multiline: false,
        disabled: None,
        on_submit: None,
        common: Common::default(),
    }
}

impl TextInput {
    /// Hint text shown when the field is empty. Also used as the default accessibility label.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Font size in logical pixels.
    pub fn size(mut self, size: f32) -> Self {
        self.style.size = size;
        self
    }

    /// Enables a taller multiline plain-text field.
    pub fn multiline(mut self) -> Self {
        self.multiline = true;
        self
    }

    /// Disables editing and dims the field, static or reactive.
    pub fn disabled(mut self, disabled: impl IntoProp<bool>) -> Self {
        self.disabled = Some(disabled.into_prop());
        self
    }

    /// Runs `f` with the current text when Enter is pressed.
    pub fn on_submit(mut self, f: impl Fn(String) + 'static) -> Self {
        self.on_submit = Some(Rc::new(f));
        self
    }
}

common_modifiers!(TextInput);

impl View for TextInput {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let get_theme = cx.theme_getter();
        let hovered = signal(false);
        let pressed = signal(false);
        let focused = signal(false);
        let value = self.value;
        let initial_len = value.with_untracked(String::len);

        let id = cx.node(
            LayoutStyle {
                padding: Insets::symmetric(12.0, 16.0),
                min_width: Some(220.0),
                min_height: self.multiline.then_some(88.0),
                ..Default::default()
            },
            Content::TextInput(TextInputState {
                value,
                placeholder: self.placeholder,
                style: self.style,
                cursor: initial_len,
                anchor: initial_len,
                preedit: None,
                on_submit: self.on_submit,
                cursor_rect: Rect::default(),
                scroll_x: 0.0,
            }),
        );
        cx.with_tree(|tree| {
            let n = &mut tree.nodes[id];
            n.role = Some(Role::TextInput);
            n.interaction = Some(Interaction {
                hovered,
                pressed,
                focused,
                focusable: true,
                on_click: None,
                on_drag: None,
                on_key: None,
                on_scroll: None,
            });
            n.visual.radius.snap(16.0);
            n.visual.border_width = 1.5;
        });

        if let Some(disabled) = self.disabled {
            cx.bind(id, disabled, |tree, id, d| {
                tree.nodes[id].disabled = d;
            });
        }

        cx.bind(
            id,
            Prop::Dynamic(Rc::new(move || value.get())),
            |_, _, _| {},
        );

        cx.bind(
            id,
            Prop::Dynamic(Rc::new(move || {
                let t = get_theme();
                let is_focused = focused.get();
                let is_hovered = hovered.get();
                let bg = if is_focused {
                    t.surface
                } else if is_hovered {
                    t.surface.mix(t.ink, 0.02)
                } else {
                    t.surface.mix(t.ink, 0.04)
                };
                let border = if is_focused {
                    t.accent
                } else if is_hovered {
                    t.ink.with_alpha(0.24)
                } else {
                    t.border
                };
                let ring = if is_focused { 1.0 } else { 0.0 };
                let elev = if is_focused { 4.0 } else { 1.0 };
                (bg, border, t.ink, t.accent, ring, elev)
            })),
            |tree, id, (bg, border, ink, tint, ring, elev)| {
                let n = &mut tree.nodes[id];
                n.visual.shadow_tint = tint;
                Node::animate(n.laid_out, &mut n.visual.background, bg);
                Node::animate(n.laid_out, &mut n.visual.border_color, border);
                Node::animate(n.laid_out, &mut n.visual.text_color, ink);
                Node::animate(n.laid_out, &mut n.visual.focus_ring, ring);
                Node::animate(n.laid_out, &mut n.visual.elevation, elev);
            },
        );
        self.common.apply(cx, id);
        id
    }
}

// ---- Stack (row / column / grid / zstack) -----------------------------------------------

type BuildChildren = Box<dyn FnOnce(&mut BuildCx) -> Vec<NodeId>>;

/// A layout container (`row`, `column()`, `grid` or `zstack`).
pub struct Stack {
    style: LayoutStyle,
    overlay_children: bool,
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
        overlay_children: false,
        children: Box::new(move |cx| children.build_children(cx)),
        common: Common::default(),
    }
}

/// Lays children out horizontally (left to right in LTR, right to left in RTL), vertically centred.
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

/// Lays children out in an equal-column grid with `columns` tracks.
pub fn grid(columns: u16, children: impl Children + 'static) -> Stack {
    Stack {
        style: LayoutStyle {
            grid_columns: Some(columns.max(1)),
            align: Align::Stretch,
            ..Default::default()
        },
        overlay_children: false,
        children: Box::new(move |cx| children.build_children(cx)),
        common: Common::default(),
    }
}

/// Stacks children on top of each other (the first child sizes the box; later children overlay it).
pub fn zstack(children: impl Children + 'static) -> Stack {
    Stack {
        style: LayoutStyle {
            align: Align::Center,
            justify: Justify::Center,
            ..Default::default()
        },
        overlay_children: true,
        children: Box::new(move |cx| children.build_children(cx)),
        common: Common::default(),
    }
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

    /// Sets layout direction (`Ltr` or `Rtl`) on this container.
    pub fn direction(mut self, direction: Direction) -> Self {
        self.style.direction = direction;
        self
    }
}

common_modifiers!(Stack);

impl View for Stack {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let id = cx.node(self.style, Content::None);
        let children = (self.children)(cx);
        if self.overlay_children {
            for &child in children.iter().skip(1) {
                cx.with_tree(|t| {
                    t.update_style(child, |s| s.absolute_fill = true);
                });
            }
        }
        cx.set_children(id, children);
        self.common.apply(cx, id);
        id
    }
}

// ---- Image ------------------------------------------------------------------------------

/// Displays an RGBA8 [`ImageData`] scaled to its layout box (clipped to corner radius).
pub struct Image {
    data: ImageData,
    common: Common,
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Image")
            .field("width", &self.data.width())
            .field("height", &self.data.height())
            .finish()
    }
}

/// Creates an [`Image`] widget from [`ImageData`].
pub fn image(data: ImageData) -> Image {
    Image {
        data,
        common: Common::default(),
    }
}

common_modifiers!(Image);

impl View for Image {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let w = f64::from(self.data.width());
        let h = f64::from(self.data.height());
        let data = self.data;
        let id = cx.node(
            LayoutStyle {
                width: Length::Px(w),
                height: Length::Px(h),
                ..Default::default()
            },
            Content::Custom(Rc::new(move |p, rect, _| {
                p.draw_image(rect, 0.0, &data);
            })),
        );
        cx.with_tree(|t| {
            t.nodes[id].role = Some(Role::Image);
        });
        self.common.apply(cx, id);
        id
    }
}
