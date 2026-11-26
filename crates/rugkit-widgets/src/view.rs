//! The [`View`] trait, the build context and child lists.

use rugkit_layout::LayoutStyle;
use rugkit_reactive::effect;

use crate::prop::Prop;
use crate::tree::{Content, NodeId, Tree, TreeHandle};

/// Something that can be built into the view tree.
///
/// A view is built once. After that, reactive properties (closures passed to setters) keep
/// the built nodes up to date through fine-grained effects. Nothing is diffed or rebuilt.
pub trait View: 'static {
    /// Builds the view's nodes and returns the root node.
    fn build(self, cx: &mut BuildCx) -> NodeId;

    /// Erases the view's type, so views of different types can share a `Vec`.
    fn into_any(self) -> AnyView
    where
        Self: Sized,
    {
        AnyView(Box::new(move |cx| self.build(cx)))
    }
}

/// A type-erased [`View`]. Create one with [`View::into_any`].
pub struct AnyView(Box<dyn FnOnce(&mut BuildCx) -> NodeId>);

impl std::fmt::Debug for AnyView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AnyView")
    }
}

impl View for AnyView {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        (self.0)(cx)
    }
}

/// Passed to [`View::build`]: creates nodes and binds reactive properties to them.
pub struct BuildCx {
    pub(crate) tree: TreeHandle,
}

impl std::fmt::Debug for BuildCx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BuildCx")
    }
}

impl BuildCx {
    pub(crate) fn node(&mut self, style: LayoutStyle, content: Content) -> NodeId {
        self.tree.with(|t| t.add(style, content))
    }

    pub(crate) fn set_children(&mut self, parent: NodeId, children: Vec<NodeId>) {
        self.tree.with(|t| t.set_children(parent, children));
    }

    pub(crate) fn with_tree<R>(&mut self, f: impl FnOnce(&mut Tree) -> R) -> R {
        self.tree.with(f)
    }

    /// Applies `prop` to node `id` now. If the prop is reactive, also re-applies it whenever
    /// the signals it reads change.
    pub(crate) fn bind<T: 'static>(
        &mut self,
        id: NodeId,
        prop: Prop<T>,
        apply: impl Fn(&mut Tree, NodeId, T) + 'static,
    ) {
        match prop {
            Prop::Static(value) => self.tree.with(|t| apply(t, id, value)),
            Prop::Dynamic(f) => {
                let tree = self.tree.clone();
                effect(move || {
                    let value = f();
                    tree.with(|t| {
                        if t.nodes.contains_key(id) {
                            apply(t, id, value);
                            t.mark_needs_frame();
                        }
                    });
                });
            }
        }
    }
}

/// A list of child views: a tuple of views, or a `Vec` of one view type.
pub trait Children {
    /// Builds every child and returns their nodes in order.
    fn build_children(self, cx: &mut BuildCx) -> Vec<NodeId>;
}

impl Children for () {
    fn build_children(self, _cx: &mut BuildCx) -> Vec<NodeId> {
        Vec::new()
    }
}

impl<V: View> Children for Vec<V> {
    fn build_children(self, cx: &mut BuildCx) -> Vec<NodeId> {
        self.into_iter().map(|v| v.build(cx)).collect()
    }
}

macro_rules! tuple_children {
    ($($name:ident),+) => {
        impl<$($name: View),+> Children for ($($name,)+) {
            #[allow(non_snake_case)]
            fn build_children(self, cx: &mut BuildCx) -> Vec<NodeId> {
                let ($($name,)+) = self;
                vec![$($name.build(cx)),+]
            }
        }
    };
}

tuple_children!(A);
tuple_children!(A, B);
tuple_children!(A, B, C);
tuple_children!(A, B, C, D);
tuple_children!(A, B, C, D, E);
tuple_children!(A, B, C, D, E, F);
tuple_children!(A, B, C, D, E, F, G);
tuple_children!(A, B, C, D, E, F, G, H);
tuple_children!(A, B, C, D, E, F, G, H, I);
tuple_children!(A, B, C, D, E, F, G, H, I, J);
tuple_children!(A, B, C, D, E, F, G, H, I, J, K);
tuple_children!(A, B, C, D, E, F, G, H, I, J, K, L);
