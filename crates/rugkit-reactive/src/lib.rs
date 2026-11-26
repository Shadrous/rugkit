//! Fine-grained reactivity for rugkit.
//!
//! - [`Signal`] holds a value. Reading it inside a [`memo`] or [`effect`] subscribes to it.
//! - [`Memo`] is a cached derived value. It recomputes lazily, and only notifies dependents
//!   when its result actually changes.
//! - [`effect`] runs a side effect now and again whenever something it read changes.
//! - [`Owner`] groups everything created under it, so you can dispose it all at once.
//!
//! Handles are `Copy`, so you can use them in any closure without `.clone()`:
//!
//! ```
//! use rugkit_reactive::{effect, memo, signal};
//! use std::{cell::RefCell, rc::Rc};
//!
//! let count = signal(1);
//! let doubled = memo(move || count.get() * 2);
//!
//! let seen = Rc::new(RefCell::new(Vec::new()));
//! let log = seen.clone();
//! effect(move || log.borrow_mut().push(doubled.get()));
//!
//! count.set(5);
//! assert_eq!(*seen.borrow(), vec![2, 10]);
//! ```
//!
//! The runtime is single-threaded: one per thread, and rugkit uses the UI thread's.
//! Background work hands results back to the UI thread (see `resource()`, planned).
//!
//! **Disposal.** When an [`Owner`] is disposed, every signal, memo and effect created under it
//! goes away. Using a handle after that panics, naming where the value was created.

#![warn(missing_debug_implementations)]

mod runtime;

use std::any::Any;
use std::cell::RefCell;
use std::fmt;
use std::marker::PhantomData;
use std::panic::Location;
use std::rc::Rc;

use runtime::{Compute, NodeId, OwnerId};

// ---- Signal -----------------------------------------------------------------------------

/// A reactive value. `Copy`, so it can be moved into any number of closures.
///
/// Create one with [`signal`].
pub struct Signal<T: 'static> {
    id: NodeId,
    defined_at: &'static Location<'static>,
    ty: PhantomData<fn() -> T>,
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Signal<T> {}

impl<T> PartialEq for Signal<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl<T> Eq for Signal<T> {}

impl<T> fmt::Debug for Signal<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Signal")
            .field("defined_at", &format_args!("{}", self.defined_at))
            .finish()
    }
}

/// Creates a [`Signal`] holding `value`, owned by the current [`Owner`].
#[track_caller]
pub fn signal<T: 'static>(value: T) -> Signal<T> {
    Signal {
        id: runtime::create_signal(Box::new(value)),
        defined_at: Location::caller(),
        ty: PhantomData,
    }
}

impl<T: 'static> Signal<T> {
    /// Same as [`signal`].
    #[track_caller]
    pub fn new(value: T) -> Self {
        signal(value)
    }

    /// Returns a clone of the value and subscribes the running computation to it.
    #[track_caller]
    pub fn get(self) -> T
    where
        T: Clone,
    {
        self.with(T::clone)
    }

    /// Calls `f` with a reference to the value and subscribes the running computation to it.
    #[track_caller]
    pub fn with<R>(self, f: impl FnOnce(&T) -> R) -> R {
        let slot = runtime::read(self.id, self.defined_at, true);
        with_slot(&slot, self.defined_at, f)
    }

    /// Like [`get`](Self::get) but doesn't subscribe.
    #[track_caller]
    pub fn get_untracked(self) -> T
    where
        T: Clone,
    {
        self.with_untracked(T::clone)
    }

    /// Like [`with`](Self::with) but doesn't subscribe.
    #[track_caller]
    pub fn with_untracked<R>(self, f: impl FnOnce(&T) -> R) -> R {
        let slot = runtime::read(self.id, self.defined_at, false);
        with_slot(&slot, self.defined_at, f)
    }

    /// Replaces the value and notifies dependents.
    #[track_caller]
    pub fn set(self, value: T) {
        self.update(|v| *v = value);
    }

    /// Replaces the value only if it differs, so dependents aren't woken for nothing.
    #[track_caller]
    pub fn set_if_changed(self, value: T)
    where
        T: PartialEq,
    {
        if self.with_untracked(|v| *v != value) {
            self.set(value);
        }
    }

    /// Mutates the value in place and notifies dependents.
    #[track_caller]
    pub fn update(self, f: impl FnOnce(&mut T)) {
        let slot = runtime::slot(self.id, self.defined_at);
        {
            let mut guard = slot.try_borrow_mut().unwrap_or_else(|_| {
                panic!(
                    "rugkit: signal created at {} was written while it was being read or written",
                    self.defined_at
                )
            });
            let value = guard
                .as_mut()
                .and_then(|b| b.downcast_mut::<T>())
                .expect("signal slot holds its declared type");
            f(value);
        }
        runtime::notify(self.id);
    }

    /// Whether the owner of this signal is still alive.
    pub fn is_alive(self) -> bool {
        runtime::is_alive(self.id)
    }
}

#[track_caller]
fn with_slot<T: 'static, R>(
    slot: &runtime::Slot,
    defined_at: &'static Location<'static>,
    f: impl FnOnce(&T) -> R,
) -> R {
    let guard = slot.try_borrow().unwrap_or_else(|_| {
        panic!("rugkit: value created at {defined_at} was read while it was being written")
    });
    let value = guard
        .as_ref()
        .and_then(|b| b.downcast_ref::<T>())
        .expect("reactive slot holds its declared type");
    f(value)
}

// ---- Memo -------------------------------------------------------------------------------

/// A cached, derived value. `Copy`. Create one with [`memo`].
pub struct Memo<T: 'static> {
    id: NodeId,
    defined_at: &'static Location<'static>,
    ty: PhantomData<fn() -> T>,
}

impl<T> Clone for Memo<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Memo<T> {}

impl<T> fmt::Debug for Memo<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Memo")
            .field("defined_at", &format_args!("{}", self.defined_at))
            .finish()
    }
}

struct MemoCompute<T, F> {
    f: F,
    ty: PhantomData<fn() -> T>,
}

impl<T: PartialEq + 'static, F: Fn() -> T> Compute for MemoCompute<T, F> {
    fn run(&self, slot: &RefCell<Option<Box<dyn Any>>>) -> bool {
        let new = (self.f)();
        let mut guard = slot.borrow_mut();
        if let Some(old) = guard.as_ref().and_then(|b| b.downcast_ref::<T>())
            && *old == new
        {
            return false;
        }
        *guard = Some(Box::new(new));
        true
    }
}

/// Creates a [`Memo`] computed by `f`.
///
/// `f` runs lazily on first read and again only after something it read has changed.
/// Dependents are notified only if the new result differs (`PartialEq`).
#[track_caller]
pub fn memo<T: PartialEq + 'static>(f: impl Fn() -> T + 'static) -> Memo<T> {
    Memo {
        id: runtime::create_memo(Rc::new(MemoCompute { f, ty: PhantomData })),
        defined_at: Location::caller(),
        ty: PhantomData,
    }
}

impl<T: 'static> Memo<T> {
    /// Returns a clone of the value and subscribes the running computation to it.
    #[track_caller]
    pub fn get(self) -> T
    where
        T: Clone,
    {
        self.with(T::clone)
    }

    /// Calls `f` with a reference to the value and subscribes the running computation to it.
    #[track_caller]
    pub fn with<R>(self, f: impl FnOnce(&T) -> R) -> R {
        let slot = runtime::read(self.id, self.defined_at, true);
        with_slot(&slot, self.defined_at, f)
    }

    /// Like [`get`](Self::get) but doesn't subscribe.
    #[track_caller]
    pub fn get_untracked(self) -> T
    where
        T: Clone,
    {
        let slot = runtime::read(self.id, self.defined_at, false);
        with_slot(&slot, self.defined_at, T::clone)
    }
}

// ---- Effect -----------------------------------------------------------------------------

/// Handle to an effect created with [`effect`]. `Copy`.
#[derive(Copy, Clone, Debug)]
pub struct Effect {
    id: NodeId,
}

impl Effect {
    /// Whether the effect is still alive (its owner hasn't been disposed).
    pub fn is_alive(self) -> bool {
        runtime::is_alive(self.id)
    }
}

struct EffectCompute<F> {
    f: RefCell<F>,
}

impl<F: FnMut()> Compute for EffectCompute<F> {
    fn run(&self, _slot: &RefCell<Option<Box<dyn Any>>>) -> bool {
        match self.f.try_borrow_mut() {
            Ok(mut f) => f(),
            Err(_) => panic!("rugkit: an effect re-triggered itself while running"),
        }
        false
    }
}

/// Runs `f` now, and again whenever a signal or memo it read changes.
///
/// Anything `f` creates (signals, effects, cleanups) is disposed before the next run.
pub fn effect(f: impl FnMut() + 'static) -> Effect {
    Effect {
        id: runtime::create_effect(Rc::new(EffectCompute { f: RefCell::new(f) })),
    }
}

// ---- scopes -----------------------------------------------------------------------------

/// Runs `f` without subscribing the current computation to anything `f` reads.
pub fn untrack<R>(f: impl FnOnce() -> R) -> R {
    runtime::untrack(f)
}

/// Runs `f` with effects deferred until it returns, so several writes trigger one update.
pub fn batch<R>(f: impl FnOnce() -> R) -> R {
    runtime::batch(f)
}

/// Registers `f` to run when the current [`Owner`] is disposed, or when the current effect
/// re-runs.
pub fn on_cleanup(f: impl FnOnce() + 'static) {
    runtime::on_cleanup(Box::new(f));
}

/// A scope that owns reactive values. `Copy`.
///
/// Everything created while the owner is current (via [`Owner::run`]) is disposed by
/// [`Owner::dispose`], including child owners.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Owner(OwnerId);

impl Owner {
    /// Creates a new owner as a child of the current one.
    pub fn new() -> Self {
        Self(runtime::new_owner())
    }

    /// The owner that's current right now.
    pub fn current() -> Self {
        Self(runtime::current_owner())
    }

    /// The thread's root owner, which is never disposed.
    pub fn root() -> Self {
        Self(runtime::root_owner())
    }

    /// Runs `f` with this owner current.
    pub fn run<R>(self, f: impl FnOnce() -> R) -> R {
        runtime::run_in_owner(self.0, f)
    }

    /// Disposes everything this owner owns. Disposing the root owner does nothing.
    pub fn dispose(self) {
        runtime::dispose_owner(self.0);
    }

    /// Whether the owner hasn't been disposed.
    pub fn is_alive(self) -> bool {
        runtime::owner_alive(self.0)
    }
}

impl Default for Owner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
