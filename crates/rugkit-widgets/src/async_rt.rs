//! Executor-agnostic async helpers: [`spawn`] and [`resource`].
//!
//! Runs background futures on a small built-in worker thread pool (no `tokio` dependency) and
//! delivers results back to the UI thread as reactive [`Signal`] updates, waking the event
//! loop automatically via [`WakeHandle`].

use std::cell::RefCell;
use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock, mpsc};

use rugkit_reactive::{Signal, batch, signal};
use rugkit_shell::WakeHandle;

type Job = Box<dyn FnOnce() + Send + 'static>;

fn pool_sender() -> &'static Mutex<mpsc::Sender<Job>> {
    static POOL: OnceLock<Mutex<mpsc::Sender<Job>>> = OnceLock::new();
    POOL.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        let workers = std::thread::available_parallelism().map_or(2, |n| n.get().clamp(2, 4));
        for i in 0..workers {
            let rx = rx.clone();
            let _ = std::thread::Builder::new()
                .name(format!("rugkit-worker-{i}"))
                .spawn(move || {
                    loop {
                        let job = {
                            let Ok(guard) = rx.lock() else { break };
                            guard.recv()
                        };
                        match job {
                            Ok(job) => job(),
                            Err(_) => break,
                        }
                    }
                });
        }
        Mutex::new(tx)
    })
}

/// Global UI-thread wake handle + delivery queue for background tasks.
struct DeliveryQueue {
    wakers: Mutex<Vec<WakeHandle>>,
    tasks: Mutex<Vec<Box<dyn FnOnce() + Send + 'static>>>,
}

fn delivery_queue() -> &'static DeliveryQueue {
    static Q: OnceLock<DeliveryQueue> = OnceLock::new();
    Q.get_or_init(|| DeliveryQueue {
        wakers: Mutex::new(Vec::new()),
        tasks: Mutex::new(Vec::new()),
    })
}

/// Registers a window's [`WakeHandle`] so completed background resources wake the UI loop.
pub(crate) fn register_waker(wake: WakeHandle) {
    if let Ok(mut wakers) = delivery_queue().wakers.lock() {
        wakers.push(wake);
    }
}

/// Drains any completed background deliveries onto the UI thread (called by `Ui::draw`).
pub(crate) fn drain_deliveries() {
    let items: Vec<_> = {
        let Ok(mut tasks) = delivery_queue().tasks.lock() else {
            return;
        };
        std::mem::take(&mut *tasks)
    };
    if !items.is_empty() {
        batch(|| {
            for task in items {
                task();
            }
        });
    }
    // Also flush any per-thread signal bridges.
    PENDING_SIGNALS.with(|cell| {
        let mut list = cell.borrow_mut();
        list.retain_mut(|poll| !poll());
    });
}

thread_local! {
    /// UI-thread pollers that check shared completion slots and update their `Signal`.
    static PENDING_SIGNALS: RefCell<Vec<Box<dyn FnMut() -> bool>>> = const { RefCell::new(Vec::new()) };
}

fn wake_all() {
    if let Ok(wakers) = delivery_queue().wakers.lock() {
        for w in wakers.iter() {
            w.wake();
        }
    }
}

/// Runs `future` to completion on rugkit's built-in background worker pool.
///
/// Works with any executor-agnostic future without requiring `tokio`.
pub fn spawn<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    let job: Job = Box::new(move || {
        pollster::block_on(future);
    });
    if let Ok(tx) = pool_sender().lock() {
        let _ = tx.send(job);
    }
}

/// The state of an asynchronous [`Resource`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceState<T, E> {
    /// The background future is still running.
    Loading,
    /// The future resolved with `Ok(T)`.
    Ready(T),
    /// The future failed with `Err(E)`.
    Error(E),
}

/// A reactive handle to an asynchronous computation created with [`resource`].
///
/// `Copy`, just like [`Signal`]. Reading [`Resource::get`] inside a reactive closure
/// automatically updates the UI when the future completes.
pub struct Resource<T: 'static, E: 'static> {
    state: Signal<ResourceState<T, E>>,
}

impl<T, E> Clone for Resource<T, E> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T, E> Copy for Resource<T, E> {}

impl<T, E> std::fmt::Debug for Resource<T, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resource").finish_non_exhaustive()
    }
}

impl<T: Clone + 'static, E: Clone + 'static> Resource<T, E> {
    /// Returns the current [`ResourceState`] and subscribes the running computation to changes.
    pub fn get(self) -> ResourceState<T, E> {
        self.state.get()
    }

    /// True while the resource is still loading.
    pub fn is_loading(self) -> bool {
        self.state.with(|s| matches!(s, ResourceState::Loading))
    }

    /// Returns `Some(T)` if ready, subscribing to changes.
    pub fn value(self) -> Option<T> {
        self.state.with(|s| match s {
            ResourceState::Ready(v) => Some(v.clone()),
            _ => None,
        })
    }

    /// Returns `Some(E)` if errored, subscribing to changes.
    pub fn error(self) -> Option<E> {
        self.state.with(|s| match s {
            ResourceState::Error(e) => Some(e.clone()),
            _ => None,
        })
    }
}

/// Runs `future` on rugkit's background worker pool and delivers its `Result<T, E>` back to
/// the UI thread as a reactive [`Resource`].
pub fn resource<T, E, F>(future: F) -> Resource<T, E>
where
    T: Send + 'static,
    E: Send + 'static,
    F: Future<Output = Result<T, E>> + Send + 'static,
{
    let state = signal(ResourceState::<T, E>::Loading);
    let slot: Arc<Mutex<Option<Result<T, E>>>> = Arc::new(Mutex::new(None));
    let slot_bg = slot.clone();

    PENDING_SIGNALS.with(|cell| {
        cell.borrow_mut().push(Box::new(move || {
            let taken = slot.lock().ok().and_then(|mut g| g.take());
            if let Some(res) = taken {
                if state.is_alive() {
                    match res {
                        Ok(v) => state.set(ResourceState::Ready(v)),
                        Err(e) => state.set(ResourceState::Error(e)),
                    }
                }
                true
            } else {
                !state.is_alive()
            }
        }));
    });

    spawn(async move {
        let res = future.await;
        if let Ok(mut g) = slot_bg.lock() {
            *g = Some(res);
        }
        wake_all();
    });

    Resource { state }
}
