//! High-performance virtualised [`Table`] widget (100k+ rows at 120fps).
//!
//! Supports fixed-height virtualised rows, sticky column headers, column sorting, interactive
//! column resizing, single and multi-row selection (`⌘`-click, `Shift`-click, arrow keys),
//! and custom cell renderers.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::rc::Rc;

use rugkit_layout::{LayoutStyle, Length};
use rugkit_reactive::{Signal, effect, signal};
use rugkit_render::{FontFamily, FontWeight, Painter, Point, Rect, TextStyle, Vec2};
use rugkit_shell::{Key, Modifiers, PointerButton, Role};

use crate::common::{Common, common_modifiers};
use crate::prop::Prop;
use crate::tree::{Content, Interaction, Node, NodeId, PointerKind};
use crate::view::{BuildCx, View};

/// Horizontal alignment for text inside a table column.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum CellAlign {
    /// Left-aligned (default).
    #[default]
    Start,
    /// Centred.
    Center,
    /// Right-aligned (ideal for numbers and currency).
    End,
}

/// Column sort direction.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SortDirection {
    /// Ascending (`A → Z`, `0 → 9`).
    Ascending,
    /// Descending (`Z → A`, `9 → 0`).
    Descending,
}

/// Row selection mode for [`Table`].
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum SelectionMode {
    /// Rows cannot be selected.
    None,
    /// At most one row is selected at a time (default).
    #[default]
    Single,
    /// Multiple rows can be selected (`⌘`-click toggles, `Shift`-click selects ranges).
    Multiple,
}

type CellRenderer<T> = Rc<dyn Fn(&mut Painter<'_>, &T, Rect, bool)>;
type RowComparator<T> = Rc<dyn Fn(&T, &T) -> Ordering>;

/// Describes one column in a [`Table`].
pub struct Column<T: 'static> {
    /// Stable column identifier.
    pub id: String,
    /// Header title text.
    pub title: String,
    /// Initial column width in logical pixels.
    pub width: f64,
    /// Minimum width when resizing.
    pub min_width: f64,
    /// Whether clicking the header sorts by this column.
    pub sortable: bool,
    /// Whether dragging the right edge of the header resizes this column.
    pub resizable: bool,
    /// Horizontal text alignment.
    pub align: CellAlign,
    renderer: CellRenderer<T>,
    comparator: Option<RowComparator<T>>,
}

impl<T: 'static> std::fmt::Debug for Column<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Column")
            .field("id", &self.id)
            .field("title", &self.title)
            .field("width", &self.width)
            .finish_non_exhaustive()
    }
}

impl<T: 'static> Column<T> {
    /// Creates a column with `id`, header `title`, initial `width` and a plain-text extractor.
    pub fn text(
        id: impl Into<String>,
        title: impl Into<String>,
        width: f64,
        extract: impl Fn(&T) -> String + 'static,
    ) -> Self {
        let align = CellAlign::Start;
        let extract = Rc::new(extract);
        let draw_extract = extract.clone();
        Self {
            id: id.into(),
            title: title.into(),
            width: width.max(40.0),
            min_width: 60.0,
            sortable: true,
            resizable: true,
            align,
            renderer: Rc::new(move |p, row, rect, selected| {
                let t = crate::theme::theme();
                let txt = draw_extract(row);
                let style = TextStyle::new(14.0)
                    .family(FontFamily::Ui)
                    .color(if selected { t.accent } else { t.ink });
                let y = rect.y0 + ((rect.height() - 18.0) * 0.5).max(0.0);
                p.text(&txt, Point::new(rect.x0 + 12.0, y), &style);
            }),
            comparator: Some(Rc::new(move |a, b| extract(a).cmp(&extract(b)))),
        }
    }

    /// Creates a column with a custom cell renderer `draw(painter, row, cell_rect, is_selected)`.
    pub fn custom(
        id: impl Into<String>,
        title: impl Into<String>,
        width: f64,
        draw: impl Fn(&mut Painter<'_>, &T, Rect, bool) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            width: width.max(40.0),
            min_width: 60.0,
            sortable: false,
            resizable: true,
            align: CellAlign::Start,
            renderer: Rc::new(draw),
            comparator: None,
        }
    }

    /// Sets the minimum width in logical pixels when resizing.
    pub fn min_width(mut self, min_width: f64) -> Self {
        self.min_width = min_width.max(36.0);
        self
    }

    /// Enables or disables sorting on this column.
    pub fn sortable(mut self, sortable: bool) -> Self {
        self.sortable = sortable;
        self
    }

    /// Enables or disables interactive column resizing.
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Sets horizontal alignment for the header label.
    pub fn align(mut self, align: CellAlign) -> Self {
        self.align = align;
        self
    }

    /// Sets a custom sort comparator for this column (automatically enables `sortable`).
    pub fn sort_by(mut self, cmp: impl Fn(&T, &T) -> Ordering + 'static) -> Self {
        self.comparator = Some(Rc::new(cmp));
        self.sortable = true;
        self
    }
}

/// A virtualised data table with sticky headers, sorting, column resize and selection.
/// Created with [`table`].
pub struct Table<T: Clone + 'static> {
    rows: Signal<Vec<T>>,
    columns: Vec<Column<T>>,
    row_height: f64,
    header_height: f64,
    selection_mode: SelectionMode,
    selected: Option<Signal<BTreeSet<usize>>>,
    sort: Option<Signal<Option<(usize, SortDirection)>>>,
    common: Common,
}

impl<T: Clone + 'static> std::fmt::Debug for Table<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Table")
            .field("columns", &self.columns.len())
            .field("row_height", &self.row_height)
            .field("selection_mode", &self.selection_mode)
            .finish_non_exhaustive()
    }
}

/// Creates a virtualised [`Table`] backed by `rows` and `columns`.
pub fn table<T: Clone + 'static>(rows: Signal<Vec<T>>, columns: Vec<Column<T>>) -> Table<T> {
    Table {
        rows,
        columns,
        row_height: 36.0,
        header_height: 40.0,
        selection_mode: SelectionMode::Single,
        selected: None,
        sort: None,
        common: Common::default(),
    }
}

impl<T: Clone + 'static> Table<T> {
    /// Fixed height of every body row in logical pixels (default `36.0`).
    pub fn row_height(mut self, height: f64) -> Self {
        self.row_height = height.max(20.0);
        self
    }

    /// Height of the sticky header row in logical pixels (default `40.0`).
    pub fn header_height(mut self, height: f64) -> Self {
        self.header_height = height.max(24.0);
        self
    }

    /// Configures single, multiple or no row selection.
    pub fn selection_mode(mut self, mode: SelectionMode) -> Self {
        self.selection_mode = mode;
        self
    }

    /// Binds selected row indices (original indices into `rows`) to an external signal.
    pub fn selected(mut self, selected: Signal<BTreeSet<usize>>) -> Self {
        self.selected = Some(selected);
        self
    }

    /// Binds active column sort state `Option<(column_index, SortDirection)>` to an external signal.
    pub fn sort(mut self, sort: Signal<Option<(usize, SortDirection)>>) -> Self {
        self.sort = Some(sort);
        self
    }
}

common_modifiers!(Table<T> where T: Clone + 'static);

struct ResizeDrag {
    col_idx: usize,
    start_x: f64,
    start_width: f64,
}

impl<T: Clone + 'static> View for Table<T> {
    fn build(self, cx: &mut BuildCx) -> NodeId {
        let get_theme = cx.theme_getter();
        let rows = self.rows;
        let row_h = self.row_height;
        let header_h = self.header_height;
        let selection_mode = self.selection_mode;
        let selected = self.selected.unwrap_or_else(|| signal(BTreeSet::new()));
        let sort_sig = self.sort.unwrap_or_else(|| signal(None));
        let scroll_y = signal(0.0f64);
        let hovered_row = signal(None::<usize>);
        let anchor_visual_row = Rc::new(RefCell::new(None::<usize>));
        let resize_drag = Rc::new(RefCell::new(None::<ResizeDrag>));

        let initial_widths: Vec<f64> = self.columns.iter().map(|c| c.width).collect();
        let col_widths = signal(initial_widths);
        let columns = Rc::new(self.columns);

        // Cached sorted index permutation: recomputed ONLY when `rows` or `sort_sig` changes,
        // never on scroll or hover frames!
        let order: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
        let order_for_effect = order.clone();
        let cols_for_effect = columns.clone();
        let tree_for_effect = cx.tree.clone();
        effect(move || {
            let active_sort = sort_sig.get();
            rows.with(|data| {
                let mut indices: Vec<usize> = (0..data.len()).collect();
                if let Some((col_idx, dir)) = active_sort
                    && let Some(col) = cols_for_effect.get(col_idx)
                    && let Some(cmp) = &col.comparator
                {
                    indices.sort_by(|&a, &b| {
                        let o = cmp(&data[a], &data[b]);
                        match dir {
                            SortDirection::Ascending => o,
                            SortDirection::Descending => o.reverse(),
                        }
                    });
                }
                *order_for_effect.borrow_mut() = indices;
            });
            tree_for_effect.with(|t| t.mark_needs_frame());
        });

        let draw_theme = get_theme.clone();
        let draw_cols = columns.clone();
        let draw_order = order.clone();
        let id = cx.node(
            LayoutStyle {
                width: Length::Px(640.0),
                height: Length::Px(320.0),
                ..Default::default()
            },
            Content::Custom(Rc::new(move |p, rect, _| {
                let t = draw_theme();
                let widths = col_widths.get_untracked();
                let active_sort = sort_sig.get_untracked();
                let sel_set = selected.get_untracked();
                let hover_v = hovered_row.get_untracked();
                let indices = draw_order.borrow();
                let total_rows = indices.len();

                let body_top = rect.y0 + header_h;
                let body_h = (rect.height() - header_h).max(1.0);
                let total_h = (total_rows as f64) * row_h;
                let max_scroll = (total_h - body_h).max(0.0);
                let y_off = scroll_y.get_untracked().clamp(0.0, max_scroll);

                p.with_clip(rect, 16.0, |p| {
                    // 1. Virtualised visible rows (clipped to body below sticky header).
                    let body_rect = Rect::from_xywh(rect.x0, body_top, rect.width(), body_h);
                    p.with_clip(body_rect, 0.0, |p| {
                        let first = (y_off / row_h).floor() as usize;
                        let visible = (body_h / row_h).ceil() as usize + 1;
                        let last = (first + visible).min(total_rows);

                        rows.with_untracked(|data| {
                            for v_idx in first..last {
                                let data_idx = indices[v_idx];
                                let Some(row_data) = data.get(data_idx) else {
                                    continue;
                                };
                                let ry = body_top + (v_idx as f64) * row_h - y_off;
                                let row_rect = Rect::from_xywh(rect.x0, ry, rect.width(), row_h);
                                let is_sel = sel_set.contains(&data_idx);
                                let is_hov = hover_v == Some(v_idx);

                                if is_sel {
                                    p.fill_rect(row_rect, 0.0, t.accent.with_alpha(0.16));
                                } else if is_hov {
                                    p.fill_rect(row_rect, 0.0, t.ink.with_alpha(0.05));
                                } else if v_idx % 2 == 1 {
                                    p.fill_rect(row_rect, 0.0, t.ink.with_alpha(0.02));
                                }

                                // Subtle row hairline divider.
                                p.fill_rect(
                                    Rect::from_xywh(rect.x0, row_rect.y1 - 1.0, rect.width(), 1.0),
                                    0.0,
                                    t.border.with_alpha(0.45),
                                );

                                let mut cx_x = rect.x0;
                                for (col_idx, col) in draw_cols.iter().enumerate() {
                                    let w = widths.get(col_idx).copied().unwrap_or(col.width);
                                    let cell_rect = Rect::from_xywh(cx_x, ry, w, row_h);
                                    p.with_clip(cell_rect, 0.0, |p| {
                                        (col.renderer)(p, row_data, cell_rect, is_sel);
                                    });
                                    cx_x += w;
                                }
                            }
                        });

                        // Subtle scrollbar thumb.
                        if max_scroll > 1.0 && body_h > 20.0 {
                            let thumb_h = ((body_h / total_h) * body_h).clamp(24.0, body_h - 8.0);
                            let frac = (y_off / max_scroll).clamp(0.0, 1.0);
                            let thumb_y = body_top + 4.0 + frac * (body_h - thumb_h - 8.0);
                            p.fill_rect(
                                Rect::from_xywh(rect.x1 - 8.0, thumb_y, 4.0, thumb_h),
                                2.0,
                                t.ink.with_alpha(0.24),
                            );
                        }
                    });

                    // 2. Sticky header pinned at the top of the table.
                    let header_rect = Rect::from_xywh(rect.x0, rect.y0, rect.width(), header_h);
                    p.fill_rect(header_rect, 0.0, t.surface_raised);
                    p.fill_rect(
                        Rect::from_xywh(rect.x0, header_rect.y1 - 1.0, rect.width(), 1.0),
                        0.0,
                        t.border,
                    );

                    let hdr_style = TextStyle::new(13.0)
                        .family(FontFamily::Ui)
                        .weight(FontWeight::BOLD)
                        .color(t.ink_soft);
                    let mut hx = rect.x0;
                    for (col_idx, col) in draw_cols.iter().enumerate() {
                        let w = widths.get(col_idx).copied().unwrap_or(col.width);
                        let indicator = match active_sort {
                            Some((i, SortDirection::Ascending)) if i == col_idx => " ↑",
                            Some((i, SortDirection::Descending)) if i == col_idx => " ↓",
                            _ => "",
                        };
                        let label = format!("{}{indicator}", col.title);
                        let label_w = p.measure_text(&label, None, &hdr_style).width;
                        let tx = match col.align {
                            CellAlign::Start => hx + 12.0,
                            CellAlign::Center => hx + ((w - label_w) * 0.5).max(8.0),
                            CellAlign::End => (hx + w - label_w - 12.0).max(hx + 6.0),
                        };
                        let ty = rect.y0 + ((header_h - 16.0) * 0.5).max(0.0);
                        p.with_clip(Rect::from_xywh(hx, rect.y0, w, header_h), 0.0, |p| {
                            p.text(&label, Point::new(tx, ty), &hdr_style);
                        });
                        if col.resizable {
                            p.fill_rect(
                                Rect::from_xywh(hx + w - 1.0, rect.y0 + 8.0, 1.0, header_h - 16.0),
                                0.0,
                                t.border,
                            );
                        }
                        hx += w;
                    }
                });
            })),
        );

        let hovered = signal(false);
        let pressed = signal(false);
        let focused = signal(false);

        let ptr_cols = columns.clone();
        let ptr_order = order.clone();
        let ptr_anchor = anchor_visual_row.clone();
        let ptr_resize = resize_drag.clone();

        let key_order = order.clone();
        let key_anchor = anchor_visual_row;

        let scroll_order = order;

        cx.with_tree(|t| {
            let n = &mut t.nodes[id];
            n.role = Some(Role::List);
            n.a11y_label = Some("Table".into());
            n.visual.radius.snap(16.0);
            n.visual.border_width = 1.0;
            n.interaction = Some(Interaction {
                hovered,
                pressed,
                focused,
                focusable: true,
                on_click: None,
                on_drag: None,
                on_pointer: Some(Rc::new(move |kind, pt, rect, mods: Modifiers| {
                    let widths = col_widths.get_untracked();
                    match kind {
                        PointerKind::Move => {
                            if let Some(drag) = ptr_resize.borrow().as_ref() {
                                let delta = pt.x - drag.start_x;
                                let min_w = ptr_cols[drag.col_idx].min_width;
                                let next_w = (drag.start_width + delta).max(min_w);
                                col_widths.update(|ws| {
                                    if let Some(slot) = ws.get_mut(drag.col_idx) {
                                        *slot = next_w;
                                    }
                                });
                                return;
                            }
                            if pt.y > rect.y0 + header_h && pt.y <= rect.y1 {
                                let y = (pt.y - (rect.y0 + header_h)) + scroll_y.get_untracked();
                                let v_idx = (y / row_h).floor() as usize;
                                let len = ptr_order.borrow().len();
                                hovered_row.set_if_changed((v_idx < len).then_some(v_idx));
                            } else {
                                hovered_row.set_if_changed(None);
                            }
                        }
                        PointerKind::Leave => {
                            hovered_row.set_if_changed(None);
                        }
                        PointerKind::Down(PointerButton::Primary) => {
                            // Header click: either start column resize (near right edge) or cycle sort.
                            if pt.y >= rect.y0 && pt.y <= rect.y0 + header_h {
                                let mut cx_x = rect.x0;
                                for (col_idx, col) in ptr_cols.iter().enumerate() {
                                    let w = widths.get(col_idx).copied().unwrap_or(col.width);
                                    let right = cx_x + w;
                                    if col.resizable && (pt.x - right).abs() <= 6.0 {
                                        *ptr_resize.borrow_mut() = Some(ResizeDrag {
                                            col_idx,
                                            start_x: pt.x,
                                            start_width: w,
                                        });
                                        return;
                                    }
                                    if pt.x >= cx_x && pt.x < right {
                                        if col.sortable && col.comparator.is_some() {
                                            let next = match sort_sig.get_untracked() {
                                                Some((i, SortDirection::Ascending))
                                                    if i == col_idx =>
                                                {
                                                    Some((col_idx, SortDirection::Descending))
                                                }
                                                Some((i, SortDirection::Descending))
                                                    if i == col_idx =>
                                                {
                                                    None
                                                }
                                                _ => Some((col_idx, SortDirection::Ascending)),
                                            };
                                            sort_sig.set(next);
                                        }
                                        return;
                                    }
                                    cx_x = right;
                                }
                                return;
                            }

                            // Body click: row selection.
                            if selection_mode == SelectionMode::None {
                                return;
                            }
                            if pt.y > rect.y0 + header_h && pt.y <= rect.y1 {
                                let y = (pt.y - (rect.y0 + header_h)) + scroll_y.get_untracked();
                                let v_idx = (y / row_h).floor() as usize;
                                let indices = ptr_order.borrow();
                                let Some(&data_idx) = indices.get(v_idx) else {
                                    return;
                                };
                                match selection_mode {
                                    SelectionMode::None => {}
                                    SelectionMode::Single => {
                                        let mut next = BTreeSet::new();
                                        next.insert(data_idx);
                                        selected.set(next);
                                        *ptr_anchor.borrow_mut() = Some(v_idx);
                                    }
                                    SelectionMode::Multiple => {
                                        if mods.shift
                                            && let Some(anchor) = *ptr_anchor.borrow()
                                        {
                                            let lo = anchor.min(v_idx);
                                            let hi = anchor.max(v_idx);
                                            let mut next = BTreeSet::new();
                                            for &idx in &indices[lo..=hi] {
                                                next.insert(idx);
                                            }
                                            selected.set(next);
                                        } else if mods.command() {
                                            selected.update(|set| {
                                                if !set.remove(&data_idx) {
                                                    set.insert(data_idx);
                                                }
                                            });
                                            *ptr_anchor.borrow_mut() = Some(v_idx);
                                        } else {
                                            let mut next = BTreeSet::new();
                                            next.insert(data_idx);
                                            selected.set(next);
                                            *ptr_anchor.borrow_mut() = Some(v_idx);
                                        }
                                    }
                                }
                            }
                        }
                        PointerKind::Up(PointerButton::Primary) => {
                            *ptr_resize.borrow_mut() = None;
                        }
                        _ => {}
                    }
                })),
                on_key: Some(Rc::new(move |key, _| {
                    if selection_mode == SelectionMode::None {
                        return false;
                    }
                    let indices = key_order.borrow();
                    if indices.is_empty() {
                        return false;
                    }
                    let cur = (*key_anchor.borrow()).unwrap_or(0);
                    let next_v = match key {
                        Key::ArrowDown => (cur + 1).min(indices.len() - 1),
                        Key::ArrowUp => cur.saturating_sub(1),
                        Key::Home => 0,
                        Key::End => indices.len() - 1,
                        _ => return false,
                    };
                    let data_idx = indices[next_v];
                    let mut set = BTreeSet::new();
                    set.insert(data_idx);
                    selected.set(set);
                    *key_anchor.borrow_mut() = Some(next_v);
                    // Auto-scroll to keep the selected row visible.
                    let row_top = (next_v as f64) * row_h;
                    let cur_y = scroll_y.get_untracked();
                    if row_top < cur_y {
                        scroll_y.set(row_top);
                    } else if row_top + row_h > cur_y + 240.0 {
                        scroll_y.set((row_top + row_h - 240.0).max(0.0));
                    }
                    true
                })),
                on_scroll: Some(Rc::new(move |delta: Vec2| {
                    let total_h = (scroll_order.borrow().len() as f64) * row_h;
                    scroll_y.update(|y| *y = (*y - delta.y).clamp(0.0, total_h));
                })),
            });
        });

        // Keep visual style reactive to theme, scroll, selection, widths, hover and focus.
        cx.bind(
            id,
            Prop::Dynamic(Rc::new(move || {
                let _ = scroll_y.get();
                let _ = hovered_row.get();
                let _ = selected.with(BTreeSet::len);
                let _ = col_widths.with(Vec::len);
                let _ = sort_sig.get();
                let t = get_theme();
                let ring = if focused.get() { 1.0 } else { 0.0 };
                (t.surface, t.border, ring)
            })),
            |t, id, (bg, border, ring)| {
                let n = &mut t.nodes[id];
                Node::animate(n.laid_out, &mut n.visual.background, bg);
                Node::animate(n.laid_out, &mut n.visual.border_color, border);
                Node::animate(n.laid_out, &mut n.visual.focus_ring, ring);
            },
        );
        // Also subscribe to column width changes directly so resize drags redraw immediately.
        cx.bind(
            id,
            Prop::Dynamic(Rc::new(move || col_widths.get())),
            |_, _, _| {},
        );

        self.common.apply(cx, id);
        id
    }
}
