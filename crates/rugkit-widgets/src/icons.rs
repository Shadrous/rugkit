//! Bundled Lucide vector icons (ISC license), rendered through [`rugkit_render::Painter`].
//!
//! Enabled by the default-on `icons` Cargo feature.

use std::rc::Rc;

use rugkit_layout::{LayoutStyle, Length};
use rugkit_render::{Color, Path, Transform, Vec2};
use rugkit_shell::Role;

use crate::common::{Common, common_modifiers};
use crate::prop::{IntoProp, Prop};
use crate::tree::{Content, Node, NodeId};
use crate::view::{BuildCx, View};

/// Named icons from the Lucide icon set (24×24 viewBox, 2px rounded strokes).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum IconName {
    /// Checkmark (`check`).
    Check,
    /// Plus sign (`plus`).
    Plus,
    /// Minus sign (`minus`).
    Minus,
    /// Close / cross (`x`).
    X,
    /// Magnifying glass (`search`).
    Search,
    /// Downward chevron (`chevron-down`).
    ChevronDown,
    /// Rightward chevron (`chevron-right`).
    ChevronRight,
    /// Leftward chevron (`chevron-left`).
    ChevronLeft,
    /// Sun (`sun`).
    Sun,
    /// Moon (`moon`).
    Moon,
    /// Settings gear (`settings`).
    Settings,
    /// Information circle (`info`).
    Info,
    /// Warning triangle (`alert-triangle`).
    AlertTriangle,
    /// Sliders (`sliders-horizontal`).
    Sliders,
    /// Eye (`eye`).
    Eye,
    /// Layers (`layers`).
    Layers,
    /// Heart (`heart`).
    Heart,
    /// Star (`star`).
    Star,
    /// Home (`home`).
    Home,
    /// Wallet (`wallet`).
    Wallet,
    /// Credit card (`credit-card`).
    CreditCard,
    /// Upward trend (`trending-up`).
    TrendingUp,
    /// Pie chart (`pie-chart`).
    PieChart,
    /// Sort arrows (`arrow-up-down`).
    ArrowUpDown,
    /// Download (`download`).
    Download,
    /// Upload (`upload`).
    Upload,
    /// Copy (`copy`).
    Copy,
    /// Trash (`trash-2`).
    Trash,
    /// Edit pencil (`pencil`).
    Edit,
    /// Filter funnel (`filter`).
    Filter,
    /// Bell (`bell`).
    Bell,
    /// User (`user`).
    User,
    /// Menu hamburger (`menu`).
    Menu,
}

impl IconName {
    /// Returns the Lucide 24×24 SVG path data for this icon.
    pub fn svg_d(self) -> &'static str {
        match self {
            Self::Check => "M20 6 9 17l-5-5",
            Self::Plus => "M5 12h14 M12 5v14",
            Self::Minus => "M5 12h14",
            Self::X => "M18 6 6 18 M6 6l12 12",
            Self::Search => "M11 3a8 8 0 1 0 0 16 8 8 0 1 0 0-16Z M21 21l-4.3-4.3",
            Self::ChevronDown => "m6 9 6 6 6-6",
            Self::ChevronRight => "m9 18 6-6-6-6",
            Self::ChevronLeft => "m15 18-6-6 6-6",
            Self::Sun => {
                "M12 8a4 4 0 1 0 0 8 4 4 0 1 0 0-8Z M12 2v2 M12 20v2 M4.93 4.93l1.41 1.41 M17.66 17.66l1.41 1.41 M2 12h2 M20 12h2 M6.34 17.66l-1.41 1.41 M19.07 4.93l-1.41 1.41"
            }
            Self::Moon => "M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z",
            Self::Settings => {
                "M12 9a3 3 0 1 0 0 6 3 3 0 1 0 0-6Z M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9c.26.6.85 1 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1Z"
            }
            Self::Info => "M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20Z M12 16v-4 M12 8h.01",
            Self::AlertTriangle => {
                "m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z M12 9v4 M12 17h.01"
            }
            Self::Sliders => {
                "M21 4h-7 M10 4H3 M21 12h-9 M8 12H3 M21 20h-5 M12 20H3 M14 2v4 M8 10v4 M16 18v4"
            }
            Self::Eye => {
                "M2 12s3-7 10-7 10 7 10 7-3 7-10 7-10-7-10-7Z M12 9a3 3 0 1 0 0 6 3 3 0 1 0 0-6Z"
            }
            Self::Layers => "m12 2 10 5-10 5L2 7l10-5Z M2 17l10 5 10-5 M2 12l10 5 10-5",
            Self::Heart => {
                "M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z"
            }
            Self::Star => {
                "M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2Z"
            }
            Self::Home => "m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z M9 22V12h6v10",
            Self::Wallet => {
                "M19 7V4a1 1 0 0 0-1-1H5a2 2 0 0 0 0 4h15a1 1 0 0 1 1 1v4h-3a2 2 0 0 0 0 4h3a1 1 0 0 0 1-1v-2a1 1 0 0 0-1-1 M3 5v14a2 2 0 0 0 2 2h15a1 1 0 0 0 1-1v-4"
            }
            Self::CreditCard => {
                "M2 7a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V7Z M2 10h20"
            }
            Self::TrendingUp => "m22 7-8.5 8.5-5-5L2 17 M16 7h6v6",
            Self::PieChart => "M21.21 15.89A10 10 0 1 1 8 2.83 M22 12A10 10 0 0 0 12 2v10Z",
            Self::ArrowUpDown => "m21 16-4 4-4-4 M17 20V4 M3 8l4-4 4 4 M7 4v16",
            Self::Download => "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4 M7 10l5 5 5-5 M12 15V3",
            Self::Upload => "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4 M17 8l-5-5-5 5 M12 3v12",
            Self::Copy => {
                "M8 10a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2V10Z M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"
            }
            Self::Trash => {
                "M3 6h18 M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6 M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2 M10 11v6 M14 11v6"
            }
            Self::Edit => "M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z",
            Self::Filter => "M22 3H2l8 9.46V19l4 2v-8.54L22 3Z",
            Self::Bell => {
                "M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9 M10.3 21a1.94 1.94 0 0 0 3.4 0"
            }
            Self::User => {
                "M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2 M12 3a4 4 0 1 0 0 8 4 4 0 1 0 0-8Z"
            }
            Self::Menu => "M4 12h16 M4 6h16 M4 18h16",
        }
    }

    /// Parses this icon into a [`Path`] in its native 24×24 coordinate space.
    pub fn path(self) -> Path {
        Path::from_svg_d(self.svg_d())
    }
}

/// A vector icon widget. Created with [`icon`].
pub struct Icon {
    name: IconName,
    size: f64,
    stroke_width: f64,
    color: Option<Prop<Color>>,
    common: Common,
}

impl std::fmt::Debug for Icon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Icon")
            .field("name", &self.name)
            .field("size", &self.size)
            .finish()
    }
}

/// Renders a Lucide vector icon at 20×20 logical pixels by default.
pub fn icon(name: IconName) -> Icon {
    Icon {
        name,
        size: 20.0,
        stroke_width: 2.0,
        color: None,
        common: Common::default(),
    }
}

impl Icon {
    /// Icon width and height in logical pixels.
    pub fn size(mut self, size: f64) -> Self {
        self.size = size;
        self
    }

    /// Stroke width in the 24×24 icon coordinate space (default `2.0`).
    pub fn stroke_width(mut self, width: f64) -> Self {
        self.stroke_width = width;
        self
    }

    /// Icon stroke colour (static or reactive). Defaults to the scoped theme's `ink`.
    pub fn color(mut self, color: impl IntoProp<Color>) -> Self {
        self.color = Some(color.into_prop());
        self
    }
}

common_modifiers!(Icon);

impl View for Icon {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let path = self.name.path();
        let stroke = self.stroke_width;
        let get_theme = cx.theme_getter();
        let id = cx.node(
            LayoutStyle {
                width: Length::Px(self.size),
                height: Length::Px(self.size),
                ..Default::default()
            },
            Content::Custom(Rc::new(move |p, rect, color| {
                let s = (rect.width().min(rect.height()) / 24.0).max(0.01);
                let tx = rect.x0 + (rect.width() - 24.0 * s) * 0.5;
                let ty = rect.y0 + (rect.height() - 24.0 * s) * 0.5;
                let xform = Transform::scale_about(s, rugkit_render::Point::ZERO)
                    .then(Transform::translate(Vec2::new(tx, ty)));
                p.with_transform(xform, |p| {
                    p.stroke_path(&path, stroke, color);
                });
            })),
        );
        cx.with_tree(|t| {
            t.nodes[id].role = Some(Role::Image);
        });
        let color_prop = self
            .color
            .unwrap_or_else(|| Prop::Dynamic(Rc::new(move || get_theme().ink)));
        cx.bind(id, color_prop, |t, id, c| {
            let n = &mut t.nodes[id];
            Node::animate(n.laid_out, &mut n.visual.text_color, c);
        });
        self.common.apply(cx, id);
        id
    }
}
