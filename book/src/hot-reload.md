# Hot-Reload & Inspector

## `cargo rugkit dev`

`rugkit-cli` provides the `cargo-rugkit` binary (`cargo rugkit dev`) built around `subsecond`:

```bash
cargo run -p rugkit-cli -- dev --example counter
```

Use `rugkit::launch_reloadable(options, init_state, build_view)` so your signals are created once in `init_state` (under a long-lived `state_owner`) and **preserved across view rebuilds** whenever `build_view` is hot-patched:

```rust
rugkit::launch_reloadable(
    WindowOptions::default(),
    || signal(0i32),
    |&count| {
        column((
            text(move || format!("Count: {}", count.get())),
            button("+").on_click(move || count.update(|c| *c += 1)),
        ))
    },
)
```

When `dioxus-cli` (`dx`) is on `PATH`, `cargo rugkit dev` delegates to `dx serve --hotpatch --features hot-reload` for zero-restart Subsecond binary hot-patching. Otherwise it runs a built-in `notify` file watcher that rebuilds and restarts automatically on save.

## In-App Inspector (`⌘⌥I`)

Press `⌘⌥I` (enabled by default in debug builds, or via `Ui::inspector_enabled(true)`) to toggle the inspector overlay:

- **`B`** — toggle layout bounds outlines on every node
- **`S`** — cycle slow-motion animation speed (`1.0×`, `0.25×`, `0.1×`)
- **`L`** — toggle Light / Dark mode live
- **`A`** — cycle accent colour presets live
- Displays live `FPS`, `CPU ms`, `GPU ms`, `Worst ms`, node count and focused widget role/label.
