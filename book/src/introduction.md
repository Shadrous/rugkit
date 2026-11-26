# Introduction

**Rugkit — The one framework that cares about visuals.**

rugkit is a Rust desktop GUI framework built around visual polish and motion: a bold, playful design language, implicit spring animations, soft layered shadows, crisp system text, and GPU-accelerated rendering out of the box.

## Design Philosophy

- **Plain Rust builder chains.** No proc macros, no JSX DSL, no separate stylesheet files.
- **Built once, updated fine-grained.** Views are constructed once into a retained tree. Reactive closures update only the affected nodes with zero tree diffing.
- **Implicit springs everywhere.** Size, position, colour, elevation, scale, lift, opacity and focus rings spring smoothly and preserve velocity when interrupted.
- **Clean public API.** `vello`, `wgpu`, `parley`, `taffy`, `winit`, `accesskit`, `muda` and `arboard` do the heavy lifting underneath, while rugkit exposes only its own stable types.

## Quick Start

Add `rugkit` to your `Cargo.toml` and launch a window:

```rust
use rugkit::prelude::*;

fn main() -> Result<(), rugkit::Error> {
    rugkit::launch(WindowOptions::default(), || {
        let count = signal(0);
        column((
            text(move || count.get().to_string())
                .size(64.0)
                .weight(FontWeight::BLACK),
            button("Add one").on_click(move || count.update(|c| *c += 1)),
        ))
        .gap(16.0)
    })
}
```

## Running the Examples

```bash
cargo run --release --example finance       # Flagship personal finance app
cargo run --release --example canvas_chart  # Animated Canvas charts + 100k-row Table
cargo run --release --example gallery       # Full widget gallery (Light & Dark)
cargo run --release --example todo          # Interactive Todo app
cargo run --release --example counter       # Counter with springs, IME, AccessKit & menus
```
