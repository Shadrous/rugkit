//! Modifiers every widget supports: size, padding, background, radius, elevation, scale and
//! animation.

use rugkit_layout::{Insets, Length};
use rugkit_render::Color;

use crate::motion::Motion;
use crate::prop::Prop;
use crate::tree::{Node, NodeId};
use crate::view::BuildCx;

#[derive(Default)]
pub(crate) struct Common {
    pub width: Option<Prop<f64>>,
    pub height: Option<Prop<f64>>,
    pub fill_width: bool,
    pub grow: Option<f64>,
    pub padding: Option<Insets>,
    pub background: Option<Prop<Color>>,
    pub radius: Option<Prop<f64>>,
    pub elevation: Option<Prop<f64>>,
    pub shadow_tint: Option<Prop<Color>>,
    pub scale: Option<Prop<f64>>,
    pub opacity: Option<Prop<f64>>,
    pub visible: Option<Prop<bool>>,
    pub border: Option<(f64, Prop<Color>)>,
    pub motion: Option<Motion>,
    pub a11y_label: Option<String>,
    pub draggable: bool,
}

impl Common {
    /// Applies the modifiers to a built node. Runs after the widget's own defaults, so user
    /// modifiers win.
    pub fn apply(self, cx: &mut BuildCx, id: NodeId) {
        if let Some(motion) = self.motion {
            cx.with_tree(|t| t.nodes[id].visual.motion = motion);
        }
        if let Some(tint) = self.shadow_tint {
            cx.bind(id, tint, |t, id, v| {
                t.nodes[id].visual.shadow_tint = v;
            });
        }
        if let Some(label) = self.a11y_label {
            cx.with_tree(|t| t.nodes[id].a11y_label = Some(label));
        }
        if self.draggable {
            cx.with_tree(|t| t.nodes[id].draggable = true);
        }
        if self.fill_width || self.grow.is_some() || self.padding.is_some() {
            cx.with_tree(|t| {
                t.update_style(id, |s| {
                    if self.fill_width {
                        s.width = Length::Fraction(1.0);
                    }
                    if let Some(g) = self.grow {
                        s.grow = g;
                        s.shrink = true;
                    }
                    if let Some(p) = self.padding {
                        s.padding = p;
                    }
                });
            });
        }
        if let Some(w) = self.width {
            cx.bind(id, w, |t, id, v| {
                t.update_style(id, |s| s.width = Length::Px(v));
            });
        }
        if let Some(h) = self.height {
            cx.bind(id, h, |t, id, v| {
                t.update_style(id, |s| s.height = Length::Px(v));
            });
        }
        if let Some(bg) = self.background {
            cx.bind(id, bg, |t, id, v| {
                let n = &mut t.nodes[id];
                Node::animate(n.laid_out, &mut n.visual.background, v);
            });
        }
        if let Some(r) = self.radius {
            cx.bind(id, r, |t, id, v| {
                let n = &mut t.nodes[id];
                Node::animate(n.laid_out, &mut n.visual.radius, v);
            });
        }
        if let Some(e) = self.elevation {
            cx.bind(id, e, |t, id, v| {
                let n = &mut t.nodes[id];
                Node::animate(n.laid_out, &mut n.visual.elevation, v);
            });
        }
        if let Some(s) = self.scale {
            cx.bind(id, s, |t, id, v| {
                let n = &mut t.nodes[id];
                Node::animate(n.laid_out, &mut n.visual.scale, v);
            });
        }
        if let Some(op) = self.opacity {
            cx.bind(id, op, |t, id, v| {
                let n = &mut t.nodes[id];
                Node::animate(n.laid_out, &mut n.visual.opacity, v.clamp(0.0, 1.0));
            });
        }
        if let Some(vis) = self.visible {
            cx.bind(id, vis, |t, id, show| {
                t.update_style(id, |s| s.hidden = !show);
            });
        }
        if let Some((w, color)) = self.border {
            cx.with_tree(|t| t.nodes[id].visual.border_width = w);
            cx.bind(id, color, |t, id, c| {
                let n = &mut t.nodes[id];
                Node::animate(n.laid_out, &mut n.visual.border_color, c);
            });
        }
    }
}

/// Generates the shared modifier methods on a widget builder that has a `common: Common` field.
macro_rules! common_modifiers {
    ($ty:ident) => {
        common_modifiers!(@impl [] $ty [] []);
    };
    ($ty:ident < $($gen:ident),+ > where $($wh:tt)+) => {
        common_modifiers!(@impl [$($gen),+] $ty [$($gen),+] [where $($wh)+]);
    };
    (@impl [$($gen:ident),*] $ty:ident [$($args:ident),*] [$($wh:tt)*]) => {
        impl<$($gen),*> $ty<$($args),*> $($wh)* {
            /// Fixed width in logical pixels (static or reactive). Changes animate.
            pub fn width(mut self, width: impl $crate::IntoProp<f64>) -> Self {
                self.common.width = Some(width.into_prop());
                self
            }

            /// Fixed height in logical pixels (static or reactive). Changes animate.
            pub fn height(mut self, height: impl $crate::IntoProp<f64>) -> Self {
                self.common.height = Some(height.into_prop());
                self
            }

            /// Fills the parent's width.
            pub fn fill_width(mut self) -> Self {
                self.common.fill_width = true;
                self
            }

            /// Takes this share of the parent's spare space along its main axis.
            pub fn grow(mut self, grow: f64) -> Self {
                self.common.grow = Some(grow);
                self
            }

            /// Space inside the edges.
            pub fn padding(mut self, padding: impl Into<$crate::Insets>) -> Self {
                self.common.padding = Some(padding.into());
                self
            }

            /// Background colour (static or reactive). Changes animate.
            pub fn background(
                mut self,
                color: impl $crate::IntoProp<rugkit_render::Color>,
            ) -> Self {
                self.common.background = Some(color.into_prop());
                self
            }

            /// Corner radius (static or reactive). Large values make a pill.
            pub fn radius(mut self, radius: impl $crate::IntoProp<f64>) -> Self {
                self.common.radius = Some(radius.into_prop());
                self
            }

            /// Elevation: how high the surface floats, drawn as layered soft shadows.
            /// `0` is flat, `4`–`10` is typical (static or reactive).
            pub fn elevation(mut self, elevation: impl $crate::IntoProp<f64>) -> Self {
                self.common.elevation = Some(elevation.into_prop());
                self
            }

            /// Colour the ambient part of the shadow is tinted with (static or reactive).
            pub fn shadow_tint(
                mut self,
                tint: impl $crate::IntoProp<rugkit_render::Color>,
            ) -> Self {
                self.common.shadow_tint = Some(tint.into_prop());
                self
            }

            /// Scale about the centre (static or reactive). Doesn't affect layout.
            pub fn scale(mut self, scale: impl $crate::IntoProp<f64>) -> Self {
                self.common.scale = Some(scale.into_prop());
                self
            }

            /// Opacity from `0.0` (transparent) to `1.0` (opaque), static or reactive.
            pub fn opacity(mut self, opacity: impl $crate::IntoProp<f64>) -> Self {
                self.common.opacity = Some(opacity.into_prop());
                self
            }

            /// Shows (`true`) or collapses out of layout (`false`) reactively.
            pub fn visible(mut self, visible: impl $crate::IntoProp<bool>) -> Self {
                self.common.visible = Some(visible.into_prop());
                self
            }

            /// Draws a rounded border of `width` logical pixels in `color` (static or reactive).
            pub fn border(
                mut self,
                width: f64,
                color: impl $crate::IntoProp<rugkit_render::Color>,
            ) -> Self {
                self.common.border = Some((width, color.into_prop()));
                self
            }

            /// The spring geometry changes use (frame, radius, elevation, scale).
            pub fn animation(mut self, spring: $crate::Spring) -> Self {
                self.common.motion = Some($crate::motion::Motion::Spring(spring));
                self
            }

            /// Turns implicit animation off: changes apply instantly.
            pub fn no_animation(mut self) -> Self {
                self.common.motion = Some($crate::motion::Motion::Instant);
                self
            }

            /// Explicit accessible label for screen readers (VoiceOver) and the test harness.
            pub fn a11y_label(mut self, label: impl Into<String>) -> Self {
                self.common.a11y_label = Some(label.into());
                self
            }

            /// Marks this region as draggable so pressing on it moves the window (for custom
            /// transparent titlebars). Interactive children inside still receive clicks.
            pub fn draggable(mut self) -> Self {
                self.common.draggable = true;
                self
            }
        }
    };
}

pub(crate) use common_modifiers;
