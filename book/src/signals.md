# Signals & Reactivity

rugkit uses its own lightweight, single-threaded reactive runtime (`rugkit-reactive`) on the UI thread with glitch-free push-pull propagation.

## `Signal<T>`

Signals are `Copy` handles into the runtime arena, so you can move them into any number of closures without `.clone()`:

```rust
use rugkit::prelude::*;

let count = signal(0);
let doubled = memo(move || count.get() * 2);

effect(move || {
    println!("doubled is now {}", doubled.get());
});

count.set(5); // prints "doubled is now 10"
```

## Batching & Untracked Reads

- Use `batch(|| { a.set(1); b.set(2); })` to coalesce multiple writes into a single effect flush.
- Use `signal.get_untracked()` or `untrack(|| ...)` when reading state without subscribing.
- Use `signal.set_if_changed(v)` to avoid notifying dependents when the new value equals the old value.

## Owners & Disposal

Every signal, memo and effect belongs to an `Owner`. When an `Owner` is disposed, all of its reactive nodes and `on_cleanup` hooks are cleaned up automatically. Accessing a disposed handle panics with a clear message naming where the signal was created.

For thread-global signals that should outlive individual views (such as system appearance), use `Owner::root()`.

## Automatic Window Waking

When a signal changes outside a pointer or keyboard event (for example from a background `resource` or timer), any bound view effect automatically wakes the window's event loop through `WakeHandle`. Idle windows draw zero frames.
