# Async & Resources

rugkit's reactive core runs single-threaded on the UI thread, while async work runs off-thread without forcing a specific runtime (`tokio` is not required).

## `spawn(future)`

Runs any `Send + 'static` future on rugkit's built-in worker thread pool:

```rust
use rugkit::prelude::*;

spawn(async move {
    // Perform file I/O or network work off the UI thread.
});
```

## `resource(async { ... })`

`resource` runs a fallible future off-thread and returns a `Copy` `Resource<T, E>` handle backed by a `Signal<ResourceState<T, E>>`. When the future finishes, rugkit delivers the result back to the UI thread and wakes the event loop via `WakeHandle`:

```rust
let data = resource(async {
    Ok::<_, String>(load_local_transactions())
});

text(move || match data.get() {
    ResourceState::Loading => "Loading…".into(),
    ResourceState::Ready(rows) => format!("Loaded {} rows", rows.len()),
    ResourceState::Error(err) => format!("Error: {err}"),
})
```
