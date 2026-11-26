//! Properties that are either fixed or reactive.

use std::rc::Rc;

use rugkit_render::Color;

/// A property value: either fixed, or a closure re-evaluated whenever the signals it reads
/// change.
pub enum Prop<T> {
    /// A fixed value.
    Static(T),
    /// A reactive value.
    Dynamic(Rc<dyn Fn() -> T>),
}

impl<T> std::fmt::Debug for Prop<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Static(_) => f.write_str("Prop::Static"),
            Self::Dynamic(_) => f.write_str("Prop::Dynamic"),
        }
    }
}

/// Converts plain values and closures into a [`Prop`], so every widget setter accepts either:
///
/// ```
/// use rugkit_reactive::signal;
/// use rugkit_widgets::{text, Prop, IntoProp};
///
/// let count = signal(0);
/// let fixed = text("Hello");
/// let live = text(move || format!("Count: {}", count.get()));
/// ```
pub trait IntoProp<T> {
    /// Performs the conversion.
    fn into_prop(self) -> Prop<T>;
}

impl<T, F: Fn() -> T + 'static> IntoProp<T> for F {
    fn into_prop(self) -> Prop<T> {
        Prop::Dynamic(Rc::new(self))
    }
}

impl<T> IntoProp<T> for Prop<T> {
    fn into_prop(self) -> Prop<T> {
        self
    }
}

macro_rules! static_prop {
    ($($t:ty),*) => {$(
        impl IntoProp<$t> for $t {
            fn into_prop(self) -> Prop<$t> {
                Prop::Static(self)
            }
        }
    )*};
}

static_prop!(f64, bool, usize, Color, String);

impl IntoProp<String> for &str {
    fn into_prop(self) -> Prop<String> {
        Prop::Static(self.to_owned())
    }
}
