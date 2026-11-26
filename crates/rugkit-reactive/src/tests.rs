use super::*;
use std::cell::Cell;

fn counter() -> (Rc<Cell<u32>>, Rc<Cell<u32>>) {
    let c = Rc::new(Cell::new(0));
    (c.clone(), c)
}

#[test]
fn signal_get_set_update() {
    let s = signal(1);
    assert_eq!(s.get(), 1);
    s.set(2);
    assert_eq!(s.get(), 2);
    s.update(|v| *v += 3);
    assert_eq!(s.get_untracked(), 5);
}

#[test]
fn signals_are_copy_into_closures() {
    let s = signal(String::from("a"));
    let a = move || s.with(|v| v.len());
    let b = move || s.update(|v| v.push('b'));
    b();
    assert_eq!(a(), 2);
}

#[test]
fn effect_runs_initially_and_on_change() {
    let s = signal(0);
    let (runs, r) = counter();
    effect(move || {
        s.get();
        r.set(r.get() + 1);
    });
    assert_eq!(runs.get(), 1);
    s.set(1);
    s.set(2);
    assert_eq!(runs.get(), 3);
}

#[test]
fn memo_is_lazy_and_cached() {
    let s = signal(2);
    let (calls, c) = counter();
    let m = memo(move || {
        c.set(c.get() + 1);
        s.get() * 10
    });
    assert_eq!(calls.get(), 0, "memo must not compute before it's read");
    assert_eq!(m.get(), 20);
    assert_eq!(m.get(), 20);
    assert_eq!(calls.get(), 1);
    s.set(3);
    assert_eq!(calls.get(), 1, "still lazy after invalidation");
    assert_eq!(m.get(), 30);
    assert_eq!(calls.get(), 2);
}

#[test]
fn memo_equality_stops_propagation() {
    let s = signal(1);
    let parity = memo(move || s.get() % 2);
    let (runs, r) = counter();
    effect(move || {
        parity.get();
        r.set(r.get() + 1);
    });
    s.set(3); // parity still 1
    s.set(5);
    assert_eq!(runs.get(), 1);
    s.set(4);
    assert_eq!(runs.get(), 2);
}

#[test]
fn diamond_is_glitch_free() {
    let a = signal(1);
    let b = memo(move || a.get() + 1);
    let c = memo(move || a.get() * 2);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    effect(move || log.borrow_mut().push((b.get(), c.get())));
    a.set(10);
    assert_eq!(*seen.borrow(), vec![(2, 2), (11, 20)]);
}

#[test]
fn batch_coalesces_writes() {
    let a = signal(0);
    let b = signal(0);
    let (runs, r) = counter();
    effect(move || {
        a.get();
        b.get();
        r.set(r.get() + 1);
    });
    batch(|| {
        a.set(1);
        b.set(1);
        a.set(2);
    });
    assert_eq!(runs.get(), 2);
}

#[test]
fn dynamic_dependencies_are_retracked() {
    let use_a = signal(true);
    let a = signal(0);
    let b = signal(0);
    let (runs, r) = counter();
    effect(move || {
        if use_a.get() {
            a.get();
        } else {
            b.get();
        }
        r.set(r.get() + 1);
    });
    b.set(1);
    assert_eq!(runs.get(), 1, "b isn't a dependency yet");
    use_a.set(false);
    assert_eq!(runs.get(), 2);
    a.set(1);
    assert_eq!(runs.get(), 2, "a is no longer a dependency");
    b.set(2);
    assert_eq!(runs.get(), 3);
}

#[test]
fn untrack_does_not_subscribe() {
    let a = signal(0);
    let (runs, r) = counter();
    effect(move || {
        untrack(|| a.get());
        r.set(r.get() + 1);
    });
    a.set(1);
    assert_eq!(runs.get(), 1);
}

#[test]
fn effect_writing_other_signal_cascades() {
    let a = signal(1);
    let b = signal(0);
    effect(move || b.set(a.get() * 100));
    let seen = Rc::new(Cell::new(0));
    let s = seen.clone();
    effect(move || s.set(b.get()));
    a.set(2);
    assert_eq!(seen.get(), 200);
}

#[test]
fn dispose_stops_effects_and_runs_cleanups() {
    let s = signal(0);
    let owner = Owner::new();
    let (runs, r) = counter();
    let (cleanups, c) = counter();
    owner.run(|| {
        effect(move || {
            s.get();
            r.set(r.get() + 1);
        });
        on_cleanup(move || c.set(c.get() + 1));
    });
    s.set(1);
    assert_eq!(runs.get(), 2);
    owner.dispose();
    assert!(!owner.is_alive());
    assert_eq!(cleanups.get(), 1);
    s.set(2);
    assert_eq!(runs.get(), 2, "disposed effect must not run");
}

#[test]
fn effect_rerun_disposes_previous_run() {
    let s = signal(0);
    let (cleanups, c) = counter();
    effect(move || {
        s.get();
        let c = c.clone();
        on_cleanup(move || c.set(c.get() + 1));
    });
    assert_eq!(cleanups.get(), 0);
    s.set(1);
    assert_eq!(cleanups.get(), 1);
    s.set(2);
    assert_eq!(cleanups.get(), 2);
}

#[test]
fn nested_owners_dispose_children() {
    let parent = Owner::new();
    let inner = parent.run(|| {
        let child = Owner::new();
        child.run(|| signal(1))
    });
    assert!(inner.is_alive());
    parent.dispose();
    assert!(!inner.is_alive());
}

#[test]
#[should_panic(expected = "after its owner was disposed")]
fn using_disposed_signal_panics_with_location() {
    let owner = Owner::new();
    let s = owner.run(|| signal(1));
    owner.dispose();
    s.get();
}

#[test]
fn slots_are_reused_safely() {
    let owner = Owner::new();
    let old = owner.run(|| signal(1));
    owner.dispose();
    let new = signal(2);
    assert!(!old.is_alive());
    assert_eq!(new.get(), 2);
}

#[test]
fn set_if_changed_skips_equal_values() {
    let s = signal(1);
    let (runs, r) = counter();
    effect(move || {
        s.get();
        r.set(r.get() + 1);
    });
    s.set_if_changed(1);
    assert_eq!(runs.get(), 1);
    s.set_if_changed(2);
    assert_eq!(runs.get(), 2);
}

#[test]
fn root_owner_survives_child_disposal() {
    let child = Owner::new();
    let root_sig = child.run(|| Owner::root().run(|| signal(42)));
    child.dispose();
    assert!(root_sig.is_alive());
    assert_eq!(root_sig.get(), 42);
}

#[test]
fn tracker_notifies_when_tracked_signal_changes() {
    let a = signal(1);
    let b = signal(2);
    let (dirty, d) = counter();
    let tracker = Tracker::new(move || d.set(d.get() + 1));

    let sum = tracker.track(|| a.get() + b.get());
    assert_eq!(sum, 3);
    assert_eq!(dirty.get(), 0);

    a.set(10);
    assert_eq!(dirty.get(), 1);

    // Re-track reading only `b`; writing `a` should no longer notify.
    let val = tracker.track(|| b.get());
    assert_eq!(val, 2);
    a.set(20);
    assert_eq!(dirty.get(), 1);
    b.set(30);
    assert_eq!(dirty.get(), 2);
}
