# Testing & Accessibility

Every rugkit widget exports its semantic role, label, checked state, numeric range and bounds through `A11yTree`. That single tree powers both **macOS VoiceOver** (via AccessKit) and rugkit's **headless `Harness`**.

## Headless Interaction Tests

`Harness` drives your view tree without opening an OS window, with an isolated in-memory clipboard:

```rust
use rugkit::{Harness, Key, Role, button, column, signal, text};

let count = signal(0);
let mut app = Harness::new(move || {
    column((
        text(move || count.get().to_string()),
        button("+").on_click(move || count.update(|c| *c += 1)),
    ))
});

app.click_label("+");
assert!(app.find(Role::Label, "1").is_some());

// Also test the exact VoiceOver / AccessKit action path:
let plus = app.find(Role::Button, "+").unwrap();
app.a11y_click(plus.id);
assert!(app.find(Role::Label, "2").is_some());
```

`Harness` also supports `press_key`, `type_text`, `ime_preedit`, `ime_commit`, `scroll`, `set_appearance`, `set_reduce_motion` and `reload_view`.

## Non-Blocking Pixel Snapshots

`check_snapshot` renders a `Harness` offscreen on the GPU via `OffscreenRenderer` and compares RGBA8 output against a baseline with a per-channel tolerance. Diffs are reported to stderr for inspection and CI artifacts without failing the build.
