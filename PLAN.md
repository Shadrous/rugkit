# Plan: rugkit

## Goal
Build **rugkit**, a public Rust GUI framework that Rust developers pick over egui, iced, Slint, Dioxus, gpui and Xilem because apps built with it **look and move beautifully out of the box**. Visual polish and motion are the main selling point. v1 targets macOS, with a portable core, and ships as crates.io `0.1` together with a polished personal finance flagship app.

## Decisions

### Identity & project
- **Tagline:** "Rugkit - The one framework that cares about visuals".
- **Name:** `rugkit`. (`rugs` is taken on crates.io by a dormant Haskell project, and `rugui` by another Rust GUI library for games.) The names `rugkit`, the `rugkit-*` sub-crates and `cargo-rugkit` are free as of 2026-11-24.
- **License:** MIT OR Apache-2.0.
- **Audience:** public framework for Rust developers building general-purpose desktop apps.
- **Who and how much time:** solo, part-time, no deadline. Milestones decide progress, not dates. 0.1 ships when the flagship looks right.
- **Going public:** repo public from day one. Publish crates.io `0.1` when the flagship demo is polished on macOS, labelled "macOS only for now".

### Platform
- **v1 targets macOS only.** The core avoids macOS-only APIs so Windows and Linux can follow.
- **Build on proven crates.** rugkit's own work is the API, reactivity, widgets, styling, motion and polish.

### Architecture
- **UI model:** retained widget tree with fine-grained reactivity (signals). Widgets keep their identity across updates, so animations work naturally.
- **Reactivity:** rugkit's own small runtime, single-threaded on the UI thread. It has signals, memos, effects and owners/scopes, and is tied into frame scheduling, animation ticks and layout invalidation.
- **Signal ownership:** signals are Copy handles into a runtime arena, so closures need no `.clone()`. Using a disposed signal panics, checked at runtime.
- **View syntax:** plain Rust builder chains, e.g. `button("Save").primary().on_click(...)`. No proc macros, no JSX-like macro and no separate DSL file.
- **Renderer:** vello on wgpu/Metal, hidden behind rugkit's own stable `Painter` API. No vello or wgpu types appear in the public API.
- **Async:** works with any executor. `spawn(future)` runs on a built-in pool or on the user's runtime (tokio, smol and so on). `resource(async { ... })` delivers results to the UI thread as signals, with loading and error states.
- **Code layout:** Cargo workspace of focused crates plus a `rugkit` facade crate that re-exports everything. Users add one dependency. Crates: `rugkit-reactive`, `rugkit-render`, `rugkit-layout`, `rugkit-widgets`, `rugkit-shell` (added in M0, see Assumptions) and `rugkit-cli`.

### Look & motion
- **Design language:** rugkit's own signature design language in a **bold/playful** direction: saturated colours, big radii, bouncy and exaggerated motion. Light and dark themes. No glass or backdrop blur.
- **Motion:** implicit springs by default. Any change to style or layout (size, position, colour, opacity) animates with an interruptible spring that keeps its velocity. A widget opts out with `.animation(none)`. The OS "Reduce Motion" setting is respected.
- **Theming:** typed design tokens (colours, radii, spacing, type scale, shadows, spring presets) in a reactive Rust struct. Any subtree can override tokens. Widgets also take per-instance modifiers such as `.radius(12).tint(...)`. Every token is animatable: switching light/dark or the accent colour animates.
- **Typography:** system fonts. SF Pro Rounded for UI text (labels, buttons, headings), regular SF Pro for dense content (Table cells, inputs).
- **Icons:** the Lucide icon set (ISC license), bundled.

### v1 widgets
- **Core set:** Text, Button, TextInput (single-line and multiline plain text), Checkbox, Toggle, Slider, Radio, Select/Dropdown, Row/Column/Stack/Grid, Scroll, virtualised List, Image, Modal, Popover, Tooltip, Menu/ContextMenu, Tabs.
- **Table (scope capped):** virtualised rows that stay smooth at 100k+, column sort, column resize, single and multi-row selection, sticky header, custom cell views.
- **Canvas:** `canvas(|p| { ... })` takes a draw closure that uses the rugkit `Painter` (paths, fills, strokes, gradients, text, images). The closure re-runs automatically when the signals it reads change. Pointer and hover events come with hit-testing helpers.

### Platform integration (all in v1)
- Accessibility through AccessKit (VoiceOver). Every widget exposes its role, label and state.
- IME and international input: CJK composition, emoji picker, dead keys, RTL text.
- Keyboard navigation and focus: tab order, focus rings, shortcuts, arrow keys in lists and tables.
- Clipboard, native macOS menu bar and context menus.
- Multi-window, with windows sharing signals and state.
- Custom transparent titlebar: content extends under it, traffic lights are inset, and apps can mark draggable regions.
- Follows the system light/dark appearance and macOS accent colour, animated on change.
- RTL layout mirroring.

### Dev tooling
- **In-app inspector** behind a hotkey: layout bounds, widget tree, live token editing, slow-motion animation, frame timing.
- **Hot-reload:** a `cargo rugkit dev` CLI built on the `subsecond` crate. It watches files and hot-patches changed functions. Affected view builders re-run and **all signal state is kept**.

### Quality
- **Testing:**
  - Unit tests for the reactive core, layout and animation maths.
  - A headless interaction harness that simulates clicks and keys and finds widgets by role or label through the accessibility tree. It doubles as the test API for rugkit users.
  - Lightweight pixel snapshot tests of key widget states, rendered offscreen. They run locally and in CI and report diffs but **never fail the build**.
- **Performance targets (hard v1 acceptance criteria, on an M-series Mac):**
  - 120fps (frames under 8ms) on ProMotion while animating, including while a 100k-row Table scrolls
  - input-to-photon latency under 1 frame
  - cold start to first frame under 250ms
  - hello-world release binary under 8MB

### Docs & release
- rustdoc on every public item, enforced with `#![deny(missing_docs)]`.
- An mdBook guide built around the concepts: signals, views, styling, motion, async.
- An `examples/` folder: counter, todo, widget gallery and canvas chart.
- **Flagship: a personal finance app.** It has a transactions Table, Canvas spending charts, budget forms, an import modal and sidebar navigation, and runs on local data with fake seed data. A recorded demo video goes in the README.

## Scope
**In (v1 / 0.1):** everything under Decisions.

**Out of 0.1:**
- Windows, Linux, web (WASM), iOS and Android
- Raw wgpu escape hatch for Canvas (planned after 0.1)
- Glass, translucency and backdrop blur
- Rich text editor
- Table extras: inline editing, grouping, frozen columns, column reordering
- JSX-like macro, separate DSL file, CSS-like stylesheets
- Bundled fonts, a bundled tokio runtime
- Blocking pixel snapshot tests
- A native look-alike widget style

**Cut line (moves to 0.2 first if 0.1 hasn't shipped after about 12 months):**
1. Hot-reload CLI (the inspector stays)
2. Multi-window (0.1 ships a single window, but the API is designed for many windows; see Assumptions)

## Steps
Order: vertical slice first. Prove that the "slick" feel works on a few widgets before building breadth.

- **M0: Pixels on screen.** Set up the workspace, CI and licence files. A winit window draws vello shapes and parley-shaped text through the first version of `Painter`.
  *Done when:* a window shows rounded rects, a soft shadow and crisp SF Pro Rounded text at 120fps.
- **M1: The slick slice.** Build the reactive runtime (signals, memos, effects, owners), taffy layout, the retained tree, the builder API, implicit springs, Reduce Motion support, and Button + Text + Row/Column in the first draft of the design language.
  *Done when:* a counter app animates every state change and interruptions with springs and already feels premium. Unit tests cover the runtime and spring maths.
- **M2: Platform plumbing.** Focus and keyboard navigation, IME, clipboard, AccessKit, native menus, the custom titlebar and following the system appearance. The headless harness starts here.
  *Done when:* VoiceOver can operate the counter app, CJK input works in a TextInput, and the harness can drive it.
- **M3: Breadth.** The rest of the core widget set, the typed token theming system with subtree overrides and modifiers, Lucide icons, RTL mirroring, multi-window, the inspector, the gallery example and snapshot tests.
  *Done when:* the gallery shows every widget in every state, in light and dark.
- **M4: Table + Canvas.** The capped Table, the reactive Painter Canvas with hit-testing, and the canvas chart example.
  *Done when:* a 100k-row Table scrolls at 120fps and the charts animate between data sets.
- **M5: Hot-reload.** `cargo rugkit dev` on subsecond, with state kept across patches. (First item on the cut line.)
- **M6: Flagship & release.** The personal finance app, the mdBook guide, a rustdoc completeness pass, the counter and todo examples, a check of every perf target, the demo video, then crates.io `0.1`.

## Assumptions (my calls, not yours: check these)
- **Text:** parley (Linebender, the same ecosystem as vello) rather than cosmic-text.
- **Supporting crates:**
  - windowing: winit
  - accessibility: `accesskit` + `accesskit_winit`
  - native menus: `muda`
  - clipboard: `arboard` or the native pasteboard
  - layout: taffy (flexbox + grid)
- **Rust:** edition 2024. MSRV policy is "latest stable minus 2 releases".
- **Hosting & CI:** GitHub with GitHub Actions macOS (Apple Silicon) runners. Snapshot diffs are uploaded as CI artifacts.
- **Multi-window API from day one:** the `App`/`Window` API and signal ownership are designed for many windows even if multi-window is cut, so adding it later breaks nothing.
- **Disposed signals:** using one panics with a message naming the signal and its owner. Debug builds carry extra creation-site tracking.
- **Reduce Motion:** springs become short crossfades or instant changes instead of being removed completely.
- **Table rows:** v1 uses fixed row heights for virtualisation. Variable row heights are out.
- **Icons:** Lucide icons are rendered as vector paths through `Painter`, behind a default-on `icons` cargo feature.
- **Inspector hotkey:** ⌘⌥I, available in debug builds only by default.
- **Built-in async pool:** a small thread pool with no tokio dependency.
- **Snapshot tests:** they render the gallery example's widget states offscreen through vello, with a small per-pixel tolerance to absorb GPU noise.
- **Flagship persistence:** a local JSON file. No database dependency.
- **Folder name:** the project folder stays `rugs/`. Only the crate names use `rugkit`.

- **Repo:** [github.com/Shadrous/rugkit](https://github.com/Shadrous/rugkit), public, created 2026-11-24 under the owner's account. It can move to a `rugkit` organisation later; GitHub redirects transferred repos.
- **M1 API shape** (my calls):
  - Reactive props are closures, via `IntoProp` (`.width(move || …)`).
  - Each widget gets shared modifiers: width, height, padding, background, radius, elevation, shadow tint, scale, `animation(Spring)`, `no_animation()`.
  - Elevation is drawn as two layered shadows: a contact shadow plus a wide one tinted by the shadow tint.
  - Spring presets: `BOUNCY` (0.45s, damping 0.62) for geometry, `SNAPPY` for colours. Under Reduce Motion, geometry snaps and colours do a short fade.
  - `row`/`column` centre children on the cross axis by default.
- **Extra crate `rugkit-shell`** (added in M0): it holds the window, GPU surface and frame loop (winit + wgpu), so platform code stays out of the facade and the other crates. The facade `rugkit` only re-exports.
- **Dev machine:** an M1 with a 60Hz display (no ProMotion), so 120fps can't be seen directly on it. The target is checked by measuring frame time (CPU work plus approximate GPU time under 8ms) with `FrameStats`, then confirmed on a ProMotion Mac before 0.1.

## Status
- **M0: Pixels on screen.** Visuals done (2026-11-24); the frame-time target isn't met yet.
  - Done: the `m0_pixels` demo shows rounded rects, layered soft shadows, gradients, a moving element, and crisp SF Pro Rounded and SF Pro text. Variable weights 300–900 all render.
  - Measured on a base M1 at a 2356×1572 physical window: average frame 7.4ms (CPU work plus waiting for the GPU to finish), worst frame about 8.5ms, CPU work about 2.5ms. The average is inside the 8ms budget; the worst frame is about 0.5ms over.
  - Not yet seen at 120Hz, because this Mac has a 60Hz display.
  - Known perf follow-ups: text is re-laid out every frame (no layout cache yet), and each glyph run allocates a small `Vec`. Profile the GPU side (large blurred shadows, the full-window gradient) before M1's frame scheduling lands.
- **M1: The slick slice.** Done (2026-11-24).
  - New crates:
    - `rugkit-reactive`: Copy signals, memos, effects, owners and batching, with push-pull glitch-free propagation.
    - `rugkit-layout`: taffy behind rugkit types.
    - `rugkit-widgets`: retained tree, `Animated` springs, theme draft, `text`/`button`/`row`/`column`, `Ui`.
  - The shell now has pointer input, on-demand frames (an idle window draws nothing) and macOS Reduce Motion. Render gained `Transform`/`with_transform` and a text layout cache (fixes the M0 follow-up of re-laying out text every frame).
  - Tests: 16 reactive (incl. diamond, dynamic deps, disposal), 7 spring maths (analytic vs integrated, frame-rate independence, retargeting), 3 headless `Ui` interaction tests (hover/press/click, drag-off cancel, layout spring, Reduce Motion snap) and 3 layout tests.
  - Counter demo while animating, on the base M1 at 1438×1333 physical: 5.2ms average frame, 6.5ms worst. That's within the 8ms budget.
  - Known gaps carried forward:
    - ~~There's no way yet to wake the UI when a signal changes outside an input event (timers, async).~~ Fixed in M2 via `WakeHandle`.
    - Children are static: no dynamic or keyed lists yet.
    - Hit-testing ignores scale/lift transforms.
    - ~~The theme is a fixed constant (reactive tokens are M3).~~ System light/dark + accent are now reactive in M2; full subtree token overrides arrive in M3.
- **M2: Platform plumbing.** Done (2026-11-26).
  - `rugkit-shell`:
    - `WakeHandle` (`EventLoopProxy`) so reactive effects, timers and async tasks wake an idle window.
    - Keyboard (`Key`, `Modifiers`), `ImeEvent` + `ImeState` cursor area sync, window focus, `Appearance` (light/dark + macOS `NSColor::controlAccentColor`), `WindowCommand::DragWindow` + macOS transparent fullsize titlebar, `clipboard` (`arboard`), native `MenuBar` (`muda`), and `A11yTree` / `A11yAction` bridged to `accesskit` + `accesskit_winit` (zero external types in public API).
  - `rugkit-render`: rounded `Painter::with_clip`, `text_x_for_byte` and `text_byte_for_x`.
  - `rugkit-reactive`: `Owner::root()` for thread-global signals (like system appearance).
  - `rugkit-widgets`:
    - Reactive `Theme::PLAYFUL_LIGHT` / `Theme::PLAYFUL_DARK` + system accent colour, animated on OS appearance change.
    - Keyboard focus traversal (`Tab` / `Shift+Tab`), animated focus rings, `Enter` / `Space` button activation, `.a11y_label(...)` and `.draggable()` modifiers.
    - `TextInput` (`text_input(signal)`) with SF Pro, caret/selection rendering, clipboard shortcuts (`⌘C`/`⌘X`/`⌘V`/`⌘A`), CJK IME preedit/commit + cursor rect reporting, and AccessKit `SetValue`/`ReplaceSelectedText`.
    - Public headless `Harness` that queries widgets by `Role` or label via `A11yTree` and drives pointer, keyboard, IME, AccessKit (VoiceOver), menu, appearance and isolated clipboard tests.
  - Counter demo updated with transparent titlebar, draggable header, native menu bar (`⌘=`/`⌘-`/`⌘0`), reactive dark/light + accent theme, and a `TextInput` note field. Measured on the base M1 while animating: ~0.62ms CPU, ~5.7ms average GPU (within the 8ms budget).
  - Tests: 17 reactive, 3 layout, 2 render, 14 widgets (including 4 new M2 headless harness tests covering VoiceOver/keyboard counter operation, CJK IME + clipboard in `TextInput`, animated appearance switching, and draggable titlebar + menu actions) + 16 doctests.
- **M3: Breadth.** Done (2026-11-26).
  - `rugkit-layout`: CSS Grid (`grid_columns`), `absolute_fill` overlays, `hidden` collapse, and RTL mirroring (`Direction::Rtl`).
  - `rugkit-render`: `Path` (including SVG `d` parsing), `ImageData`, `Painter::with_opacity`, `fill_path`, `stroke_path`, `stroke_line`, `draw_image`, and GPU `OffscreenRenderer` + `compare_rgba` for non-blocking pixel snapshot tests.
  - `rugkit-shell`: multi-window (`run_windows`, `WindowSpec`, `App::take_new_windows`), mouse wheel / trackpad `InputEvent::Scroll`, and expanded AccessKit roles (`Checkbox`, `Switch`, `Slider`, `RadioButton`, `ComboBox`, `Tab`, `TabList`, `Dialog`, `List`, `ScrollView`, `Image`, `Tooltip`) with checked & numeric state.
  - `rugkit-widgets`:
    - Typed token theming (`Theme`, `Radii`, `Spacing`, `TypeScale`) with reactive subtree overrides (`theme_scope`) and RTL scopes (`direction_scope`).
    - Full core widget set: `checkbox`, `toggle`, `slider`, `radio`, `select`, `scroll`, `virtual_list` (100k+ rows), `tabs`, `modal`, `popover`, `tooltip`, `image`, `grid`, `zstack`, multiline/disabled `text_input`, and bundled Lucide vector icons (`icon(IconName::...)`, behind default-on `icons` feature).
    - In-app `⌘⌥I` inspector (layout bounds overlay, slow-motion time scale, live Light/Dark & accent preset switching, focus & frame timing HUD).
    - Non-blocking offscreen pixel snapshot helper (`check_snapshot`).
  - `gallery` example (`cargo run --release --example gallery`) visually verified in both Light and Dark mode (`docs/screenshots/m3_gallery_light.png`, `docs/screenshots/m3_gallery_dark.png`).
  - Tests: 17 reactive, 5 layout, 2 render, 16 widgets (including full M3 controls/modal/tabs/slider/virtual-list test and offscreen GPU snapshot test) + 16 doctests.
- M4–M6: not started.

## Open questions
- A logo, and whether to move the repo to a GitHub organisation.
- The actual design tokens of the bold/playful language (palette, radii, spring presets), worked out visually during M1.
- Whether to ask the dormant owner of `rugs` on crates.io to transfer the name, for a possible later rename.
