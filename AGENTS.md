# AGENTS.md

Guidance for AI coding agents (and humans) working on **rugkit**: *Rugkit - The one framework that cares about visuals.*

rugkit is a Rust GUI framework whose main selling point is visual polish and motion. **[PLAN.md](PLAN.md) holds every product decision, the scope, the milestones (M0–M6) and their status.** Read it before making any non-trivial change.

## Rules

### 1. Keep documentation up to date (mandatory)
Documentation changes go in **the same change** as the code. A task isn't done until the docs match the code. When you change behaviour, the public API, architecture, commands or the crate layout, update everything that describes it:

- **rustdoc** on every public item you add or touch (`#![deny(missing_docs)]` is on in every crate). Doc examples must stay correct.
- **README.md** when the pitch, quick start, requirements or status change.
- **AGENTS.md** (this file) when the workspace layout, commands or conventions change.
- **PLAN.md** when:
  - a milestone's status changes (update the Status section)
  - an assumption gets confirmed or replaced
  - a new crate, dependency or deviation from the plan is introduced. Record it and say why.
- The **mdBook guide** (`book/`, from M6) when a concept it explains changes.

Before you finish, check every doc touched by the area you changed and fix anything stale, including docs written by someone else.

### 2. Decisions in PLAN.md belong to the project owner
Don't silently change a decision listed under *Decisions* or *Scope*. If one blocks you or looks wrong, stop and ask. You may refine items under *Assumptions*, but record the change in PLAN.md.

### 3. Keep the public API clean
- No vello, wgpu, parley, fontique, kurbo, peniko or winit types in rugkit's public API. rugkit owns its own types (`Painter`, `Color`, `Rect`, …) and converts internally. The one planned exception is the post-0.1 `wgpu-interop` hatch.
- Users write builder chains in plain Rust. Add no proc macros to the user-facing API.

### 4. Keep the core portable
v1 runs only on macOS, but only `rugkit-shell` may contain platform-specific code, behind `#[cfg(target_os = "macos")]` with a fallback that compiles elsewhere. All other crates must build on any OS.

### 5. Polish and performance are features
- Hard targets (M-series Mac): frames under 8ms while animating, cold start under 250ms, hello-world release binary under 8MB. Don't add work to the per-frame path without measuring it. `FrameStats` exists for this.
- Visual defaults should look premium without tuning: soft layered shadows, big radii, crisp text, spring motion.

### 6. Scope discipline
- Build things in milestone order (see PLAN.md). Don't create crates or features before the milestone that needs them, and don't add empty placeholder crates.
- Anything out of scope for 0.1 (see PLAN.md) needs the owner's approval.

## Workspace layout

```
Cargo.toml                 workspace root (edition 2024, shared deps and lints)
crates/
  rugkit/                  facade crate: the one dependency users add; re-exports the others
    examples/m0_pixels.rs  M0 demo: shapes, shadows, gradients, text, frame stats
  rugkit-render/           Painter API, geometry, colour, text; vello + parley inside
  rugkit-shell/            window, GPU surface and frame loop (winit + wgpu); platform code lives here
.github/workflows/ci.yml   fmt, clippy, tests, docs on a macOS runner
docs/screenshots/          images used by README (window-only captures)
PLAN.md                    product decisions, scope, milestones, status
```

Planned crates (create them only when their milestone arrives): `rugkit-reactive` (M1), `rugkit-layout` (M1), `rugkit-widgets` (M1/M3), `rugkit-cli` / `cargo-rugkit` (M5).

## Commands

```bash
cargo run --release --example m0_pixels   # M0 demo window
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

All of these must pass before a change counts as done. CI runs the same checks.

## Conventions
- Rust edition 2024. MSRV is set in the workspace `Cargo.toml`, following a "latest stable minus 2" policy.
- Dependency versions live in `[workspace.dependencies]`, and crates use `dep.workspace = true`.
- Coordinates in the public API are logical pixels (`f64`). Converting to physical pixels happens inside the renderer.
- Colours are non-premultiplied sRGB (`Color`).
- License: MIT OR Apache-2.0. Every crate sets `license.workspace = true`.
