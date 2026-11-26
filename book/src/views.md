# Views, Layout, Table & Canvas

Views in rugkit are plain Rust builder chains that implement `View`. They are built **once** into a retained tree; reactive setters take closures (`impl IntoProp<T>`) that bind fine-grained effects directly to the node.

## Layout Containers

- `row((a, b, c)).gap(12.0)` — horizontal flexbox (automatically mirrors in RTL).
- `column((a, b, c)).gap(12.0)` — vertical flexbox.
- `grid(3, (...)).gap(14.0)` — equal-column CSS Grid.
- `zstack((base, overlay))` — stacked layers (used for modals and badges).
- `scroll(child).height(400.0)` — clipped scroll viewport with spring scrolling.
- `virtual_list(100_000, 32.0, |p, i, rect| ...)` — fixed-height virtualised list.

## Core Widgets

- `text`, `button`, `text_input` (single-line & `.multiline()`, with CJK IME & clipboard)
- `checkbox`, `toggle`, `slider`, `radio`, `select`, `tabs`
- `modal`, `popover`, `tooltip`, `image`, `icon(IconName::...)`

## Virtualised `Table`

`table(rows, columns)` handles 100k+ fixed-height rows smoothly with sticky headers, cached column sorting, interactive column resizing, single and multi-row selection (`⌘`-click, `Shift`-click, arrow keys), and custom cell renderers:

```rust
let cols = vec![
    Column::text("merchant", "Merchant", 220.0, |r: &Tx| r.merchant.clone())
        .sort_by(|a, b| a.merchant.cmp(&b.merchant)),
    Column::custom("amount", "Amount", 140.0, |p, r: &Tx, rect, _| {
        // Draw custom badges, sparklines, or right-aligned currency
    })
    .align(CellAlign::End),
];
table(rows, cols).selection_mode(SelectionMode::Multiple)
```

## Reactive `Canvas`

`canvas(|p, rect| { ... })` gives you direct access to rugkit's `Painter` (paths, fills, strokes, gradients, text, images, clipping, opacity). Any `Signal` or `Memo` read inside the draw closure is automatically tracked via `Tracker`, so the canvas redraws whenever your data changes. Pair it with `HitRegions<T>` for effortless chart hover and click hit-testing.
