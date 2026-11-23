//! Layout for rugkit.
//!
//! Flexbox layout computed by [taffy](https://docs.rs/taffy), behind rugkit's own types, so
//! taffy never appears in rugkit's public API. Units are logical pixels.
//!
//! ```
//! use rugkit_layout::{Axis, LayoutStyle, LayoutTree, Length};
//!
//! let mut tree = LayoutTree::new();
//! let a = tree.add_leaf(LayoutStyle { width: Length::Px(40.0), height: Length::Px(20.0), ..Default::default() }, 0);
//! let b = tree.add_leaf(LayoutStyle { width: Length::Px(60.0), height: Length::Px(20.0), ..Default::default() }, 1);
//! let row = tree.add(LayoutStyle { axis: Axis::Row, gap: 10.0, ..Default::default() });
//! tree.set_children(row, &[a, b]);
//! tree.compute(row, (500.0, 500.0), |_, _, _| (0.0, 0.0));
//! assert_eq!(tree.rect(b).x, 50.0);
//! ```

use taffy::prelude::{
    AlignItems, AvailableSpace, Dimension, FlexDirection, JustifyContent, LengthPercentage,
    LengthPercentageAuto, Rect as TRect, Size as TSize, Style, TaffyTree,
};

/// Main axis of a container.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Axis {
    /// Children flow left to right.
    Row,
    /// Children flow top to bottom.
    #[default]
    Column,
}

/// Cross-axis alignment of children.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Align {
    /// Top (in a row) or left (in a column).
    Start,
    /// Centred.
    Center,
    /// Bottom (in a row) or right (in a column).
    End,
    /// Stretched to fill the cross axis.
    #[default]
    Stretch,
}

/// Main-axis distribution of children.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Justify {
    /// Packed at the start.
    #[default]
    Start,
    /// Packed in the centre.
    Center,
    /// Packed at the end.
    End,
    /// First and last at the edges, equal space between.
    SpaceBetween,
}

/// A size along one axis.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub enum Length {
    /// Sized by content and flex rules.
    #[default]
    Auto,
    /// Fixed size in logical pixels.
    Px(f64),
    /// A fraction of the parent's size (`1.0` = 100%).
    Fraction(f64),
}

/// Space inside a container's edges.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Insets {
    /// Top.
    pub top: f64,
    /// Right.
    pub right: f64,
    /// Bottom.
    pub bottom: f64,
    /// Left.
    pub left: f64,
}

impl Insets {
    /// The same inset on every side.
    pub const fn all(v: f64) -> Self {
        Self {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    /// `vertical` on top and bottom, `horizontal` on left and right.
    pub const fn symmetric(vertical: f64, horizontal: f64) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }
}

impl From<f64> for Insets {
    fn from(v: f64) -> Self {
        Self::all(v)
    }
}

impl From<(f64, f64)> for Insets {
    /// `(vertical, horizontal)`.
    fn from((vertical, horizontal): (f64, f64)) -> Self {
        Self::symmetric(vertical, horizontal)
    }
}

/// How a node is laid out.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct LayoutStyle {
    /// Direction children flow in.
    pub axis: Axis,
    /// Space between children.
    pub gap: f64,
    /// Space inside the edges.
    pub padding: Insets,
    /// Cross-axis alignment of children.
    pub align: Align,
    /// Main-axis distribution of children.
    pub justify: Justify,
    /// Width.
    pub width: Length,
    /// Height.
    pub height: Length,
    /// Minimum width in logical pixels.
    pub min_width: Option<f64>,
    /// Minimum height in logical pixels.
    pub min_height: Option<f64>,
    /// How much of the parent's spare main-axis space this node takes (flex-grow).
    pub grow: f64,
    /// Whether this node may shrink below its content size when space is short.
    pub shrink: bool,
}

impl Default for LayoutStyle {
    fn default() -> Self {
        Self {
            axis: Axis::Column,
            gap: 0.0,
            padding: Insets::default(),
            align: Align::Stretch,
            justify: Justify::Start,
            width: Length::Auto,
            height: Length::Auto,
            min_width: None,
            min_height: None,
            grow: 0.0,
            shrink: false,
        }
    }
}

impl LayoutStyle {
    fn to_taffy(self) -> Style {
        let dim = |l: Length| match l {
            Length::Auto => Dimension::auto(),
            Length::Px(v) => Dimension::length(v as f32),
            Length::Fraction(f) => Dimension::percent(f as f32),
        };
        let min = |v: Option<f64>| match v {
            Some(v) => LengthPercentageAuto::length(v as f32),
            None => LengthPercentageAuto::auto(),
        };
        let lp = |v: f64| LengthPercentage::length(v as f32);
        Style {
            flex_direction: match self.axis {
                Axis::Row => FlexDirection::Row,
                Axis::Column => FlexDirection::Column,
            },
            gap: TSize {
                width: lp(self.gap),
                height: lp(self.gap),
            },
            padding: TRect {
                left: lp(self.padding.left),
                right: lp(self.padding.right),
                top: lp(self.padding.top),
                bottom: lp(self.padding.bottom),
            },
            align_items: match self.align {
                Align::Start => AlignItems::FLEX_START,
                Align::Center => AlignItems::CENTER,
                Align::End => AlignItems::FLEX_END,
                Align::Stretch => AlignItems::STRETCH,
            },
            justify_content: match self.justify {
                Justify::Start => JustifyContent::FLEX_START,
                Justify::Center => JustifyContent::CENTER,
                Justify::End => JustifyContent::FLEX_END,
                Justify::SpaceBetween => JustifyContent::SPACE_BETWEEN,
            },
            size: TSize {
                width: dim(self.width),
                height: dim(self.height),
            },
            min_size: TSize {
                width: min(self.min_width),
                height: min(self.min_height),
            },
            flex_grow: self.grow as f32,
            flex_shrink: if self.shrink { 1.0 } else { 0.0 },
            ..Style::default()
        }
    }
}

/// Opaque handle to a node in a [`LayoutTree`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct LayoutId(taffy::NodeId);

/// A node's computed box, relative to its parent's top-left corner.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct LayoutRect {
    /// Left edge relative to the parent.
    pub x: f64,
    /// Top edge relative to the parent.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

/// A tree of layout nodes. Leaves can be measured by a callback (used for text).
pub struct LayoutTree {
    taffy: TaffyTree<u64>,
}

impl std::fmt::Debug for LayoutTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LayoutTree")
            .field("nodes", &self.taffy.total_node_count())
            .finish()
    }
}

impl Default for LayoutTree {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutTree {
    /// Creates an empty tree.
    pub fn new() -> Self {
        Self {
            taffy: TaffyTree::new(),
        }
    }

    /// Adds a container node.
    pub fn add(&mut self, style: LayoutStyle) -> LayoutId {
        LayoutId(
            self.taffy
                .new_leaf(style.to_taffy())
                .expect("taffy node creation is infallible"),
        )
    }

    /// Adds a leaf whose content size comes from the measure callback passed to
    /// [`compute`](Self::compute), which receives `key`.
    pub fn add_leaf(&mut self, style: LayoutStyle, key: u64) -> LayoutId {
        LayoutId(
            self.taffy
                .new_leaf_with_context(style.to_taffy(), key)
                .expect("taffy node creation is infallible"),
        )
    }

    /// Replaces a node's style. The tree recomputes on the next [`compute`](Self::compute).
    pub fn set_style(&mut self, id: LayoutId, style: LayoutStyle) {
        let _ = self.taffy.set_style(id.0, style.to_taffy());
    }

    /// Replaces a node's children.
    pub fn set_children(&mut self, parent: LayoutId, children: &[LayoutId]) {
        let ids: Vec<taffy::NodeId> = children.iter().map(|c| c.0).collect();
        let _ = self.taffy.set_children(parent.0, &ids);
    }

    /// Marks a measured leaf as needing re-measurement (e.g. its text changed).
    pub fn mark_dirty(&mut self, id: LayoutId) {
        let _ = self.taffy.mark_dirty(id.0);
    }

    /// Removes a node.
    pub fn remove(&mut self, id: LayoutId) {
        let _ = self.taffy.remove(id.0);
    }

    /// Computes layout for the subtree at `root` within `available` (width, height).
    ///
    /// `measure(key, known, available_width)` returns a measured leaf's (width, height).
    /// `known` holds any dimension that's already fixed. `available_width` is `None` when
    /// unconstrained.
    pub fn compute(
        &mut self,
        root: LayoutId,
        available: (f64, f64),
        mut measure: impl FnMut(u64, (Option<f64>, Option<f64>), Option<f64>) -> (f64, f64),
    ) {
        let space = TSize {
            width: AvailableSpace::Definite(available.0 as f32),
            height: AvailableSpace::Definite(available.1 as f32),
        };
        let _ = self
            .taffy
            .compute_layout_with_measure(root.0, space, |inputs, _id, ctx, style| {
                let Some(key) = ctx.map(|k| *k) else {
                    return taffy::compute_leaf_layout(
                        inputs,
                        style,
                        |_, _| 0.0,
                        |_, _| TSize::ZERO,
                    );
                };
                taffy::compute_leaf_layout(
                    inputs,
                    style,
                    |_, _| 0.0,
                    |known, avail| {
                        let known = (known.width.map(f64::from), known.height.map(f64::from));
                        let avail_w = match avail.width {
                            AvailableSpace::Definite(w) => Some(f64::from(w)),
                            AvailableSpace::MinContent => Some(0.0),
                            AvailableSpace::MaxContent => None,
                        };
                        let (w, h) = measure(key, known, avail_w);
                        TSize {
                            width: w as f32,
                            height: h as f32,
                        }
                    },
                )
            });
    }

    /// The computed box of `id`, relative to its parent.
    pub fn rect(&self, id: LayoutId) -> LayoutRect {
        self.taffy
            .layout(id.0)
            .map(|l| LayoutRect {
                x: f64::from(l.location.x),
                y: f64::from(l.location.y),
                width: f64::from(l.size.width),
                height: f64::from(l.size.height),
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_centres_children() {
        let mut t = LayoutTree::new();
        let child = t.add(LayoutStyle {
            width: Length::Px(100.0),
            height: Length::Px(50.0),
            ..Default::default()
        });
        let root = t.add(LayoutStyle {
            align: Align::Center,
            justify: Justify::Center,
            width: Length::Px(300.0),
            height: Length::Px(200.0),
            ..Default::default()
        });
        t.set_children(root, &[child]);
        t.compute(root, (300.0, 200.0), |_, _, _| (0.0, 0.0));
        let r = t.rect(child);
        assert_eq!((r.x, r.y, r.width, r.height), (100.0, 75.0, 100.0, 50.0));
    }

    #[test]
    fn measured_leaf_uses_callback_and_padding() {
        let mut t = LayoutTree::new();
        let leaf = t.add_leaf(LayoutStyle::default(), 7);
        let root = t.add(LayoutStyle {
            padding: Insets::symmetric(10.0, 20.0),
            align: Align::Start,
            ..Default::default()
        });
        t.set_children(root, &[leaf]);
        t.compute(root, (500.0, 500.0), |key, _, _| {
            assert_eq!(key, 7);
            (80.0, 18.0)
        });
        let r = t.rect(leaf);
        assert_eq!((r.x, r.y, r.width, r.height), (20.0, 10.0, 80.0, 18.0));
    }

    #[test]
    fn grow_fills_remaining_space() {
        let mut t = LayoutTree::new();
        let fixed = t.add(LayoutStyle {
            width: Length::Px(100.0),
            ..Default::default()
        });
        let flex = t.add(LayoutStyle {
            grow: 1.0,
            ..Default::default()
        });
        let root = t.add(LayoutStyle {
            axis: Axis::Row,
            width: Length::Px(400.0),
            height: Length::Px(10.0),
            ..Default::default()
        });
        t.set_children(root, &[fixed, flex]);
        t.compute(root, (400.0, 10.0), |_, _, _| (0.0, 0.0));
        assert_eq!(t.rect(flex).width, 300.0);
    }
}
