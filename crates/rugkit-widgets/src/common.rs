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
    pub shadow_tint: Option<Color>,
    pub scale: Option<Prop<f64>>,
    pub motion: Option<Motion>,
}

impl Common {
    /// Applies the modifiers to a built node. Runs after the widget's own defaults, so user
    /// modifiers win.
    pub fn apply(self, cx: &mut BuildCx, id: NodeId) {
        if let Some(motion) = self.motion {
            cx.with_tree(|t| t.nodes[id].visual.motion = motion);
        }
        if let Some(tint) = self.shadow_tint {
            cx.with_tree(|t| t.nodes[id].visual.shadow_tint = tint);
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
    }
}

/// Generates the shared modifier methods on a widget builder that has a `common: Common` field.
macro_rules! common_modifiers {
    ($ty:ty) => {
        impl $ty {
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

            /// Colour the ambient part of the shadow is tinted with.
            pub fn shadow_tint(mut self, tint: rugkit_render::Color) -> Self {
                self.common.shadow_tint = Some(tint);
                self
            }

            /// Scale about the centre (static or reactive). Doesn't affect layout.
            pub fn scale(mut self, scale: impl $crate::IntoProp<f64>) -> Self {
                self.common.scale = Some(scale.into_prop());
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
        }
    };
}

pub(crate) use common_modifiers;
