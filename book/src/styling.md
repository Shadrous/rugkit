# Styling, Theming & RTL

rugkit ships a bold, playful signature design language in both Light and Dark modes (`Theme::PLAYFUL_LIGHT` and `Theme::PLAYFUL_DARK`), following macOS system appearance and accent colour automatically.

## Typed Design Tokens

`Theme` is a typed Rust struct containing:

- **Colours:** `background`, `surface`, `surface_raised`, `ink`, `ink_soft`, `accent`, `positive`, `negative`, `warning`, `border`
- **Scales:** `radii` (`sm`, `md`, `lg`, `pill`), `spacing` (`xs`..`xl`), `type_scale` (`caption`, `body`, `heading`, `title`)
- **Springs:** `layout_spring`, `color_spring`

Reading `theme()` inside a reactive closure subscribes to OS light/dark and accent changes so every colour animates smoothly.

## Subtree Theme Overrides

Wrap any subtree in `theme_scope` to override tokens locally:

```rust
theme_scope(
    |t| t.with_accent(Color::from_hex(0xFF5A5F)),
    column((
        button("Coral Accent"),
        checkbox(checked).label("Inherits coral"),
    )),
)
```

## Per-Instance Modifiers

Every widget shares builder modifiers: `.width(...)`, `.height(...)`, `.fill_width()`, `.grow(...)`, `.padding(...)`, `.background(...)`, `.radius(...)`, `.border(w, color)`, `.elevation(...)`, `.shadow_tint(...)`, `.scale(...)`, `.opacity(...)`, `.visible(...)`, `.a11y_label(...)`, `.draggable()`.

## RTL Layout Mirroring

Use `direction_scope(Direction::Rtl, child)` or `.direction(Direction::Rtl)` on containers to mirror horizontal `row` child ordering and left/right padding automatically.
