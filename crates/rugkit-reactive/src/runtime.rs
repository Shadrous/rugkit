//! The reactive graph: node storage, dependency tracking, change propagation, owners.
//!
//! Propagation uses the push-pull "colouring" scheme:
//! - Writing a signal marks its direct subscribers `Dirty` and everything downstream `Check`.
//! - Reading a memo, or flushing an effect, first brings its sources up to date. It
//!   recomputes only if something it depends on actually changed.
//!
//! This makes updates glitch-free: in a diamond, an effect runs once per change and never
//! sees a half-updated state.
//!
//! The runtime lives in a thread-local `RefCell`. That borrow is never held while user code
//! runs, so user closures can freely read and write signals.

use std::any::Any;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::panic::Location;
use std::rc::Rc;

pub(crate) type Slot = Rc<RefCell<Option<Box<dyn Any>>>>;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct NodeId {
    index: u32,
    generation: u32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct OwnerId {
    index: u32,
    generation: u32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum State {
    Clean,
    Check,
    Dirty,
}

/// A computation's body. `run` returns whether the stored value changed.
pub(crate) trait Compute {
    fn run(&self, slot: &RefCell<Option<Box<dyn Any>>>) -> bool;
}

#[derive(Clone)]
enum Kind {
    Signal,
    Memo(Rc<dyn Compute>),
    Effect(Rc<dyn Compute>),
}

struct Node {
    generation: u32,
    alive: bool,
    kind: Kind,
    value: Slot,
    state: State,
    sources: Vec<NodeId>,
    subscribers: Vec<NodeId>,
    /// The owner this node belongs to (disposed with it).
    owner: OwnerId,
    /// For computations: the owner of everything created during the last run.
    run_owner: Option<OwnerId>,
}

struct OwnerData {
    generation: u32,
    alive: bool,
    parent: Option<OwnerId>,
    children: Vec<OwnerId>,
    nodes: Vec<NodeId>,
    cleanups: Vec<Box<dyn FnOnce()>>,
}

struct Runtime {
    nodes: Vec<Node>,
    free_nodes: Vec<u32>,
    owners: Vec<OwnerData>,
    free_owners: Vec<u32>,
    root: OwnerId,
    current_owner: OwnerId,
    observer: Option<NodeId>,
    pending_effects: VecDeque<NodeId>,
    batch_depth: u32,
    flushing: bool,
}

impl Runtime {
    fn new() -> Self {
        let mut rt = Self {
            nodes: Vec::new(),
            free_nodes: Vec::new(),
            owners: Vec::new(),
            free_owners: Vec::new(),
            root: OwnerId {
                index: 0,
                generation: 0,
            },
            current_owner: OwnerId {
                index: 0,
                generation: 0,
            },
            observer: None,
            pending_effects: VecDeque::new(),
            batch_depth: 0,
            flushing: false,
        };
        let root = rt.alloc_owner(None);
        rt.root = root;
        rt.current_owner = root;
        rt
    }

    fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes
            .get(id.index as usize)
            .filter(|n| n.alive && n.generation == id.generation)
    }

    fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes
            .get_mut(id.index as usize)
            .filter(|n| n.alive && n.generation == id.generation)
    }

    fn owner_mut(&mut self, id: OwnerId) -> Option<&mut OwnerData> {
        self.owners
            .get_mut(id.index as usize)
            .filter(|o| o.alive && o.generation == id.generation)
    }

    fn alloc_owner(&mut self, parent: Option<OwnerId>) -> OwnerId {
        let data = |generation| OwnerData {
            generation,
            alive: true,
            parent,
            children: Vec::new(),
            nodes: Vec::new(),
            cleanups: Vec::new(),
        };
        let id = if let Some(index) = self.free_owners.pop() {
            let generation = self.owners[index as usize].generation + 1;
            self.owners[index as usize] = data(generation);
            OwnerId { index, generation }
        } else {
            self.owners.push(data(0));
            OwnerId {
                index: (self.owners.len() - 1) as u32,
                generation: 0,
            }
        };
        if let Some(p) = parent.and_then(|p| self.owner_mut(p)) {
            p.children.push(id);
        }
        id
    }

    fn alloc_node(&mut self, kind: Kind, value: Slot, state: State) -> NodeId {
        let owner = if self.owner_mut(self.current_owner).is_some() {
            self.current_owner
        } else {
            // Creating inside a disposed owner: attach to the root so nothing leaks silently.
            self.root
        };
        let make = |generation| Node {
            generation,
            alive: true,
            kind,
            value,
            state,
            sources: Vec::new(),
            subscribers: Vec::new(),
            owner,
            run_owner: None,
        };
        let id = if let Some(index) = self.free_nodes.pop() {
            let generation = self.nodes[index as usize].generation + 1;
            self.nodes[index as usize] = make(generation);
            NodeId { index, generation }
        } else {
            self.nodes.push(make(0));
            NodeId {
                index: (self.nodes.len() - 1) as u32,
                generation: 0,
            }
        };
        self.owner_mut(owner)
            .expect("owner checked above")
            .nodes
            .push(id);
        id
    }

    /// Marks `id` at least `state`. Queues effects and pushes `Check` downstream on first touch.
    fn mark(&mut self, id: NodeId, state: State) {
        let Some(node) = self.node_mut(id) else {
            return;
        };
        if node.state >= state {
            return;
        }
        let was_clean = node.state == State::Clean;
        node.state = state;
        if !was_clean {
            return;
        }
        let is_effect = matches!(node.kind, Kind::Effect(_));
        let subs = node.subscribers.clone();
        if is_effect {
            self.pending_effects.push_back(id);
        }
        for sub in subs {
            self.mark(sub, State::Check);
        }
    }

    fn track(&mut self, source: NodeId) {
        let Some(observer) = self.observer else {
            return;
        };
        if let Some(obs) = self.node_mut(observer)
            && !obs.sources.contains(&source)
        {
            obs.sources.push(source);
            if let Some(src) = self.node_mut(source)
                && !src.subscribers.contains(&observer)
            {
                src.subscribers.push(observer);
            }
        }
    }
}

thread_local! {
    static RT: RefCell<Runtime> = RefCell::new(Runtime::new());
}

fn with_rt<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    RT.with(|rt| f(&mut rt.borrow_mut()))
}

#[track_caller]
fn stale(defined_at: &'static Location<'static>) -> ! {
    panic!("rugkit: a reactive value created at {defined_at} was used after its owner was disposed")
}

// ---- nodes -----------------------------------------------------------------------------

pub(crate) fn create_signal(value: Box<dyn Any>) -> NodeId {
    with_rt(|rt| {
        rt.alloc_node(
            Kind::Signal,
            Rc::new(RefCell::new(Some(value))),
            State::Clean,
        )
    })
}

pub(crate) fn create_memo(compute: Rc<dyn Compute>) -> NodeId {
    with_rt(|rt| {
        rt.alloc_node(
            Kind::Memo(compute),
            Rc::new(RefCell::new(None)),
            State::Dirty,
        )
    })
}

/// Creates an effect and runs it once immediately.
pub(crate) fn create_effect(compute: Rc<dyn Compute>) -> NodeId {
    let id = with_rt(|rt| {
        rt.alloc_node(
            Kind::Effect(compute),
            Rc::new(RefCell::new(None)),
            State::Dirty,
        )
    });
    update_if_necessary(id);
    id
}

/// Creates a manual read-tracker whose `on_dirty` runs whenever any signal read inside
/// [`track_reads`] subsequently changes.
pub(crate) fn create_tracker(on_dirty: Rc<dyn Compute>) -> NodeId {
    with_rt(|rt| {
        rt.alloc_node(
            Kind::Effect(on_dirty),
            Rc::new(RefCell::new(None)),
            State::Clean,
        )
    })
}

/// Runs `f` with `id` as the active observer, replacing its previous dependencies.
pub(crate) fn track_reads<R>(id: NodeId, f: impl FnOnce() -> R) -> R {
    let prev_observer = with_rt(|rt| {
        if let Some(node) = rt.node_mut(id) {
            let sources = std::mem::take(&mut node.sources);
            node.state = State::Clean;
            for s in sources {
                if let Some(src) = rt.node_mut(s) {
                    src.subscribers.retain(|x| *x != id);
                }
            }
            rt.observer.replace(id)
        } else {
            rt.observer
        }
    });
    let out = f();
    with_rt(|rt| {
        rt.observer = prev_observer;
    });
    out
}

/// Tracks a read of `id` by the current observer, brings memos up to date and returns the
/// value slot.
#[track_caller]
pub(crate) fn read(id: NodeId, defined_at: &'static Location<'static>, tracked: bool) -> Slot {
    let is_memo = with_rt(|rt| match rt.node(id) {
        Some(n) => matches!(n.kind, Kind::Memo(_)),
        None => stale(defined_at),
    });
    if is_memo {
        update_if_necessary(id);
    }
    with_rt(|rt| {
        if tracked {
            rt.track(id);
        }
        match rt.node(id) {
            Some(n) => n.value.clone(),
            None => stale(defined_at),
        }
    })
}

/// Returns the value slot of a signal for writing, without tracking.
#[track_caller]
pub(crate) fn slot(id: NodeId, defined_at: &'static Location<'static>) -> Slot {
    with_rt(|rt| match rt.node(id) {
        Some(n) => n.value.clone(),
        None => stale(defined_at),
    })
}

/// Call after a signal's value changed: marks dependents and runs effects (unless batching).
pub(crate) fn notify(id: NodeId) {
    with_rt(|rt| {
        let subs = rt
            .node(id)
            .map(|n| n.subscribers.clone())
            .unwrap_or_default();
        for sub in subs {
            rt.mark(sub, State::Dirty);
        }
    });
    flush_effects();
}

pub(crate) fn is_alive(id: NodeId) -> bool {
    with_rt(|rt| rt.node(id).is_some())
}

fn update_if_necessary(id: NodeId) {
    let Some((state, sources)) = with_rt(|rt| rt.node(id).map(|n| (n.state, n.sources.clone())))
    else {
        return;
    };
    if state == State::Check {
        for src in sources {
            let src_is_memo = with_rt(|rt| {
                rt.node(src)
                    .is_some_and(|n| matches!(n.kind, Kind::Memo(_)))
            });
            if src_is_memo {
                update_if_necessary(src);
            }
            if with_rt(|rt| rt.node(id).is_some_and(|n| n.state == State::Dirty)) {
                break;
            }
        }
    }
    if with_rt(|rt| rt.node(id).is_some_and(|n| n.state == State::Dirty)) {
        recompute(id);
    }
    with_rt(|rt| {
        if let Some(n) = rt.node_mut(id) {
            n.state = State::Clean;
        }
    });
}

fn recompute(id: NodeId) {
    // Detach from old sources and dispose what the previous run created.
    let Some((compute, slot, old_run_owner, node_owner)) = with_rt(|rt| {
        let node = rt.node_mut(id)?;
        let compute = match &node.kind {
            Kind::Memo(c) | Kind::Effect(c) => c.clone(),
            Kind::Signal => return None,
        };
        let sources = std::mem::take(&mut node.sources);
        let slot = node.value.clone();
        let old = node.run_owner.take();
        let owner = node.owner;
        for s in sources {
            if let Some(src) = rt.node_mut(s) {
                src.subscribers.retain(|x| *x != id);
            }
        }
        Some((compute, slot, old, owner))
    }) else {
        return;
    };
    if let Some(old) = old_run_owner {
        dispose_owner(old);
    }
    let (prev_owner, prev_observer) = with_rt(|rt| {
        let parent = if rt.owner_mut(node_owner).is_some() {
            node_owner
        } else {
            rt.root
        };
        let run_owner = rt.alloc_owner(Some(parent));
        if let Some(n) = rt.node_mut(id) {
            n.run_owner = Some(run_owner);
        }
        let prev_owner = std::mem::replace(&mut rt.current_owner, run_owner);
        let prev_observer = rt.observer.replace(id);
        (prev_owner, prev_observer)
    });

    let changed = compute.run(&slot);

    with_rt(|rt| {
        rt.current_owner = prev_owner;
        rt.observer = prev_observer;
        if changed
            && let Some(n) = rt.node(id)
            && matches!(n.kind, Kind::Memo(_))
        {
            let subs = n.subscribers.clone();
            for sub in subs {
                if let Some(s) = rt.node_mut(sub) {
                    s.state = State::Dirty;
                }
            }
        }
    });
}

fn flush_effects() {
    let start = with_rt(|rt| {
        if rt.flushing || rt.batch_depth > 0 {
            false
        } else {
            rt.flushing = true;
            true
        }
    });
    if !start {
        return;
    }
    while let Some(next) = with_rt(|rt| rt.pending_effects.pop_front()) {
        update_if_necessary(next);
    }
    with_rt(|rt| rt.flushing = false);
}

// ---- tracking scopes --------------------------------------------------------------------

pub(crate) fn untrack<R>(f: impl FnOnce() -> R) -> R {
    let prev = with_rt(|rt| rt.observer.take());
    let out = f();
    with_rt(|rt| rt.observer = prev);
    out
}

pub(crate) fn batch<R>(f: impl FnOnce() -> R) -> R {
    with_rt(|rt| rt.batch_depth += 1);
    let out = f();
    with_rt(|rt| rt.batch_depth -= 1);
    flush_effects();
    out
}

// ---- owners -----------------------------------------------------------------------------

pub(crate) fn new_owner() -> OwnerId {
    with_rt(|rt| {
        let parent = rt.current_owner;
        rt.alloc_owner(Some(parent))
    })
}

pub(crate) fn current_owner() -> OwnerId {
    with_rt(|rt| rt.current_owner)
}

pub(crate) fn root_owner() -> OwnerId {
    with_rt(|rt| rt.root)
}

pub(crate) fn run_in_owner<R>(owner: OwnerId, f: impl FnOnce() -> R) -> R {
    let prev = with_rt(|rt| std::mem::replace(&mut rt.current_owner, owner));
    let out = f();
    with_rt(|rt| rt.current_owner = prev);
    out
}

pub(crate) fn on_cleanup(f: Box<dyn FnOnce()>) {
    with_rt(|rt| {
        let owner = rt.current_owner;
        if let Some(o) = rt.owner_mut(owner) {
            o.cleanups.push(f);
        }
    });
}

pub(crate) fn owner_alive(owner: OwnerId) -> bool {
    with_rt(|rt| rt.owner_mut(owner).is_some())
}

/// Disposes an owner: its child owners, cleanups and every node created under it.
pub(crate) fn dispose_owner(owner: OwnerId) {
    let Some((children, cleanups, nodes)) = with_rt(|rt| {
        if owner == rt.root {
            return None;
        }
        let o = rt.owner_mut(owner)?;
        Some((
            std::mem::take(&mut o.children),
            std::mem::take(&mut o.cleanups),
            std::mem::take(&mut o.nodes),
        ))
    }) else {
        return;
    };
    for child in children {
        dispose_owner(child);
    }
    for cleanup in cleanups.into_iter().rev() {
        cleanup();
    }
    // Values and closures are dropped after the runtime borrow is released, because their
    // `Drop` impls may touch the runtime.
    let mut graveyard: Vec<(Slot, Kind)> = Vec::new();
    let mut run_owners = Vec::new();
    with_rt(|rt| {
        for id in nodes {
            let Some(node) = rt.node_mut(id) else {
                continue;
            };
            node.alive = false;
            let sources = std::mem::take(&mut node.sources);
            let subs = std::mem::take(&mut node.subscribers);
            if let Some(ro) = node.run_owner.take() {
                run_owners.push(ro);
            }
            let value = std::mem::replace(&mut node.value, Rc::new(RefCell::new(None)));
            let kind = std::mem::replace(&mut node.kind, Kind::Signal);
            graveyard.push((value, kind));
            for s in sources {
                if let Some(src) = rt.node_mut(s) {
                    src.subscribers.retain(|x| *x != id);
                }
            }
            for s in subs {
                if let Some(sub) = rt.node_mut(s) {
                    sub.sources.retain(|x| *x != id);
                }
            }
            rt.pending_effects.retain(|x| *x != id);
            rt.free_nodes.push(id.index);
        }
    });
    for ro in run_owners {
        dispose_owner(ro);
    }
    drop(graveyard);
    with_rt(|rt| {
        let parent = rt.owner_mut(owner).and_then(|o| {
            o.alive = false;
            o.parent
        });
        if let Some(p) = parent.and_then(|p| rt.owner_mut(p)) {
            p.children.retain(|c| *c != owner);
        }
        rt.free_owners.push(owner.index);
    });
}
