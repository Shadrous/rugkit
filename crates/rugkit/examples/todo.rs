//! Todo example: demonstrates signals, reactive filtering, TextInput with Enter-to-add,
//! Table/list interaction, implicit spring animations and Light/Dark mode.
//!
//! ```bash
//! cargo run --release --example todo
//! ```

use std::collections::BTreeSet;

use rugkit::prelude::*;

#[derive(Clone)]
struct TodoItem {
    id: usize,
    title: String,
    done: bool,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum Filter {
    All,
    Active,
    Done,
}

fn main() -> Result<(), rugkit::Error> {
    let items = signal(vec![
        TodoItem {
            id: 1,
            title: "Design bold playful tokens".into(),
            done: true,
        },
        TodoItem {
            id: 2,
            title: "Wire up implicit spring motion".into(),
            done: true,
        },
        TodoItem {
            id: 3,
            title: "Try Tab, VoiceOver and CJK IME".into(),
            done: false,
        },
        TodoItem {
            id: 4,
            title: "Ship rugkit 0.1".into(),
            done: false,
        },
    ]);
    let next_id = signal(5usize);
    let draft = signal(String::new());
    let filter = signal(Filter::All);
    let filtered_rows = signal(Vec::<TodoItem>::new());
    let selected = signal(BTreeSet::new());

    effect(move || {
        let f = filter.get();
        let list = items.with(|all| {
            all.iter()
                .filter(|item| match f {
                    Filter::All => true,
                    Filter::Active => !item.done,
                    Filter::Done => item.done,
                })
                .cloned()
                .collect::<Vec<_>>()
        });
        filtered_rows.set(list);
    });

    let add_item = move || {
        let text_val = draft.get_untracked().trim().to_string();
        if text_val.is_empty() {
            return;
        }
        let id = next_id.get_untracked();
        next_id.set(id + 1);
        items.update(|list| {
            list.push(TodoItem {
                id,
                title: text_val,
                done: false,
            });
        });
        draft.set(String::new());
    };

    let cols = vec![
        Column::custom("status", "Status", 90.0, |p, item: &TodoItem, r, _| {
            let t = theme();
            let color = if item.done { t.positive } else { t.warning };
            let pill = Rect::from_xywh(r.x0 + 10.0, r.y0 + 7.0, 68.0, r.height() - 14.0);
            p.fill_rect(pill, 999.0, color.with_alpha(0.18));
            let label = if item.done { "Done" } else { "Open" };
            let st = TextStyle::new(12.5)
                .family(FontFamily::Ui)
                .weight(FontWeight::SEMIBOLD)
                .color(color);
            let w = p.measure_text(label, None, &st).width;
            p.text(
                label,
                Point::new(pill.center().x - w * 0.5, pill.y0 + 3.0),
                &st,
            );
        })
        .sort_by(|a, b| a.done.cmp(&b.done)),
        Column::text("title", "Task", 410.0, |item: &TodoItem| item.title.clone()),
    ];

    rugkit::launch(
        WindowOptions {
            title: "rugkit · todo".into(),
            size: Size::new(620.0, 560.0),
            background: theme().background,
            transparent_titlebar: true,
            menu_bar: Some(MenuBar::standard_app("rugkit")),
            ..WindowOptions::default()
        },
        move || {
            let filter_btn = move |label: &'static str, target: Filter| {
                button(label)
                    .secondary()
                    .padding((7.0, 14.0))
                    .background(move || {
                        if filter.get() == target {
                            theme().accent
                        } else {
                            theme().surface
                        }
                    })
                    .label_color(move || {
                        if filter.get() == target {
                            Color::WHITE
                        } else {
                            theme().ink
                        }
                    })
                    .on_click(move || filter.set(target))
            };

            column((
                row((
                    icon(IconName::Check).size(22.0).color(|| theme().accent),
                    text("rugkit todo")
                        .size(22.0)
                        .weight(FontWeight::BLACK)
                        .color(|| theme().ink),
                ))
                .gap(10.0)
                .height(36.0)
                .draggable(),
                row((
                    text_input(draft)
                        .placeholder("Add a new task and press Enter…")
                        .width(380.0)
                        .on_submit(move |_| add_item()),
                    button("Add").padding((12.0, 20.0)).on_click(add_item),
                ))
                .gap(10.0),
                row((
                    row((
                        filter_btn("All", Filter::All),
                        filter_btn("Active", Filter::Active),
                        filter_btn("Done", Filter::Done),
                    ))
                    .gap(8.0),
                    row((
                        button("Toggle Selected")
                            .secondary()
                            .padding((7.0, 12.0))
                            .on_click(move || {
                                let sel: Vec<usize> =
                                    selected.with_untracked(|s| s.iter().copied().collect());
                                let ids: Vec<usize> = filtered_rows.with_untracked(|rows| {
                                    sel.iter()
                                        .filter_map(|&idx| rows.get(idx).map(|r| r.id))
                                        .collect()
                                });
                                items.update(|all| {
                                    for item in all.iter_mut() {
                                        if ids.contains(&item.id) {
                                            item.done = !item.done;
                                        }
                                    }
                                });
                            }),
                        button("Clear Done")
                            .secondary()
                            .padding((7.0, 12.0))
                            .on_click(move || {
                                items.update(|all| all.retain(|i| !i.done));
                                selected.set(BTreeSet::new());
                            }),
                    ))
                    .gap(8.0),
                ))
                .width(520.0)
                .justify(Justify::SpaceBetween),
                table(filtered_rows, cols)
                    .row_height(38.0)
                    .header_height(38.0)
                    .selection_mode(SelectionMode::Multiple)
                    .selected(selected)
                    .width(520.0)
                    .height(320.0),
                text(move || {
                    let remaining = items.with(|all| all.iter().filter(|i| !i.done).count());
                    format!("{remaining} remaining · select rows and click Toggle Selected")
                })
                .size(13.0)
                .color(|| theme().ink_soft),
            ))
            .gap(16.0)
            .padding(24.0)
        },
    )
}
