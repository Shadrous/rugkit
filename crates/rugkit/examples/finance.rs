//! Flagship Personal Finance App built with **rugkit**.
//!
//! Features:
//! - Sidebar navigation (`Overview`, `Transactions`, `Budgets`, `Settings`) with Lucide icons
//! - Reactive animated Canvas charts (category spending bar chart + net cashflow trend line)
//!   with `HitRegions` hover callouts
//! - Virtualised Transactions `Table` with search, category filter, column sort, resize and
//!   multi-row selection
//! - Interactive Budget forms with sliders, progress bars and an Add Transaction form
//! - Import Modal that imports a sample JSON batch and persists to a local JSON file
//! - Transparent macOS titlebar, native menu bar, Light/Dark & accent switching, and `⌘⌥I` inspector
//!
//! ```bash
//! cargo run --release --example finance
//! ```
//!
//! Environment variables:
//! - `FINANCE_THEME=light` or `FINANCE_THEME=dark` forces the initial theme.
//! - `FINANCE_DEMO=1` cycles through Overview → Transactions → Budgets → Import Modal for recordings.
//! - `FINANCE_PRINT_STATS=1` prints live `FrameStats` every 2 seconds.
//! - `FINANCE_EXIT_AFTER=<seconds>` exits after that many seconds.
//! - `FINANCE_DATA_PATH=<path>` overrides the local JSON persistence path.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use rugkit::prelude::*;
use rugkit::{
    A11yAction, A11yTree, Animated, AnyView, ImeState, InputEvent, Motion, Ui, WakeHandle,
    WindowCommand,
};
use serde::{Deserialize, Serialize};

const CATEGORIES: [&str; 6] = ["Groceries", "Dining", "Tech", "Travel", "Health", "Income"];

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Transaction {
    id: usize,
    merchant: String,
    category: String,
    date: String,
    amount_cents: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BudgetLimit {
    category: String,
    limit_dollars: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FinanceStore {
    transactions: Vec<Transaction>,
    budgets: Vec<BudgetLimit>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum NavSection {
    Overview,
    Transactions,
    Budgets,
    Settings,
}

fn data_path() -> PathBuf {
    if let Ok(p) = std::env::var("FINANCE_DATA_PATH") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".rugkit_finance_demo.json")
}

fn seed_store() -> FinanceStore {
    const SEED_MERCHANTS: [(&str, &str, i64); 10] = [
        ("Acme Corp Payroll", "Income", 425_000),
        ("Whole Foods Market", "Groceries", -14_850),
        ("Blue Bottle Coffee", "Dining", -875),
        ("Trader Joe's", "Groceries", -9_420),
        ("Apple Store", "Tech", -24_900),
        ("Equinox Fitness", "Health", -21_000),
        ("Delta Air Lines", "Travel", -38_400),
        ("Sweetgreen", "Dining", -1_840),
        ("Vanguard Dividend", "Income", 64_200),
        ("Oura Subscription", "Health", -599),
    ];
    let mut transactions = Vec::with_capacity(240);
    for i in 0..240 {
        let (m, cat, base) = SEED_MERCHANTS[(i * 7 + i / 5) % SEED_MERCHANTS.len()];
        let jitter = ((i * 53) % 1400) as i64;
        let amount_cents = if base >= 0 {
            base + jitter * 4
        } else {
            base - jitter
        };
        let month = (i % 6) + 1;
        let day = ((i * 3) % 28) + 1;
        transactions.push(Transaction {
            id: i + 1,
            merchant: m.to_string(),
            category: cat.to_string(),
            date: format!("2026-{month:02}-{day:02}"),
            amount_cents,
        });
    }
    let budgets = vec![
        BudgetLimit {
            category: "Groceries".into(),
            limit_dollars: 900.0,
        },
        BudgetLimit {
            category: "Dining".into(),
            limit_dollars: 450.0,
        },
        BudgetLimit {
            category: "Tech".into(),
            limit_dollars: 600.0,
        },
        BudgetLimit {
            category: "Travel".into(),
            limit_dollars: 800.0,
        },
        BudgetLimit {
            category: "Health".into(),
            limit_dollars: 400.0,
        },
    ];
    FinanceStore {
        transactions,
        budgets,
    }
}

fn load_store() -> FinanceStore {
    let path = data_path();
    if let Ok(bytes) = std::fs::read(&path)
        && let Ok(store) = serde_json::from_slice::<FinanceStore>(&bytes)
        && !store.transactions.is_empty()
    {
        return store;
    }
    seed_store()
}

fn save_store(transactions: &[Transaction], budgets: &[BudgetLimit]) {
    let store = FinanceStore {
        transactions: transactions.to_vec(),
        budgets: budgets.to_vec(),
    };
    if let Ok(json) = serde_json::to_string_pretty(&store) {
        let _ = std::fs::write(data_path(), json);
    }
}

fn category_color(cat: &str) -> Color {
    match cat {
        "Income" => Color::from_hex(0x00B894),
        "Groceries" => Color::from_hex(0x7B61FF),
        "Dining" => Color::from_hex(0xFF9F1C),
        "Tech" => Color::from_hex(0x2F80ED),
        "Travel" => Color::from_hex(0xFF5A5F),
        "Health" => Color::from_hex(0xE84393),
        _ => Color::from_hex(0x6C5CE7),
    }
}

fn format_dollars(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.abs();
    let dollars = abs / 100;
    let rem = abs % 100;
    let d_str = dollars.to_string();
    let mut with_commas = String::new();
    for (i, ch) in d_str.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            with_commas.push(',');
        }
        with_commas.push(ch);
    }
    let formatted: String = with_commas.chars().rev().collect();
    format!("{sign}${formatted}.{rem:02}")
}

fn kpi_card(
    title: &'static str,
    value: impl Fn() -> String + 'static,
    subtitle: &'static str,
    accent: impl Fn() -> Color + 'static,
    ic: IconName,
) -> impl View {
    let accent = Rc::new(accent);
    let a1 = accent.clone();
    let a2 = accent.clone();
    column((
        row((
            text(title)
                .size(13.0)
                .weight(FontWeight::SEMIBOLD)
                .color(|| theme().ink_soft),
            icon(ic).size(18.0).color(move || a1()),
        ))
        .fill_width()
        .justify(Justify::SpaceBetween),
        text(value)
            .size(26.0)
            .weight(FontWeight::BLACK)
            .letter_spacing(-0.8)
            .color(|| theme().ink),
        text(subtitle).size(12.0).color(move || a2()),
    ))
    .gap(8.0)
    .align(Align::Start)
    .padding(18.0)
    .background(|| theme().surface)
    .border(1.0, || theme().border)
    .radius(22.0)
    .elevation(4.0)
    .shadow_tint(move || accent())
}

#[allow(clippy::too_many_arguments)]
fn finance_view(
    section: Signal<NavSection>,
    transactions: Signal<Vec<Transaction>>,
    budgets: Signal<Vec<BudgetLimit>>,
    import_open: Signal<bool>,
    bar_vals: Signal<[f64; 5]>,
    dark: Signal<bool>,
    status_msg: Signal<String>,
) -> impl View {
    let search = signal(String::new());
    let cat_filter = signal(0usize); // 0 = All, 1..=6 = CATEGORIES
    let filtered_tx = signal(Vec::<Transaction>::new());
    let selected = signal(BTreeSet::new());
    let sort = signal(None::<(usize, SortDirection)>);
    let hovered_bar = signal(None::<usize>);
    let new_merchant = signal(String::new());
    let new_cat_idx = signal(0usize);
    let new_amount = signal(45.0f64);

    // Keep filtered transactions synced with search & category filter.
    effect(move || {
        let q = search.get().to_lowercase();
        let cf = cat_filter.get();
        let list = transactions.with(|all| {
            all.iter()
                .filter(|tx| {
                    let cat_ok =
                        cf == 0 || CATEGORIES.get(cf - 1).is_some_and(|c| *c == tx.category);
                    let q_ok = q.is_empty()
                        || tx.merchant.to_lowercase().contains(&q)
                        || tx.category.to_lowercase().contains(&q);
                    cat_ok && q_ok
                })
                .cloned()
                .collect::<Vec<_>>()
        });
        filtered_tx.set(list);
    });

    // ---- Sidebar Navigation ----
    let nav_btn = move |label: &'static str, target: NavSection, ic: IconName| {
        row((
            icon(ic).size(17.0).color(move || {
                if section.get() == target {
                    Color::WHITE
                } else {
                    theme().ink_soft
                }
            }),
            button(label)
                .secondary()
                .padding((8.0, 10.0))
                .background(|| Color::TRANSPARENT)
                .elevation(0.0)
                .border(0.0, Color::TRANSPARENT)
                .label_color(move || {
                    if section.get() == target {
                        Color::WHITE
                    } else {
                        theme().ink
                    }
                })
                .on_click(move || section.set(target)),
        ))
        .gap(4.0)
        .padding((2.0, 12.0))
        .fill_width()
        .justify(Justify::Start)
        .radius(14.0)
        .background(move || {
            if section.get() == target {
                theme().accent
            } else {
                Color::TRANSPARENT
            }
        })
    };

    let sidebar = column((
        // Top space for macOS inset traffic lights + draggable brand header.
        row((
            icon(IconName::Wallet).size(22.0).color(|| theme().accent),
            text("Minty")
                .size(21.0)
                .weight(FontWeight::BLACK)
                .color(|| theme().ink),
        ))
        .gap(10.0)
        .padding((28.0, 12.0))
        .fill_width()
        .justify(Justify::Start)
        .draggable(),
        column((
            nav_btn("Overview", NavSection::Overview, IconName::Home),
            nav_btn(
                "Transactions",
                NavSection::Transactions,
                IconName::CreditCard,
            ),
            nav_btn("Budgets", NavSection::Budgets, IconName::PieChart),
            nav_btn("Settings", NavSection::Settings, IconName::Settings),
        ))
        .gap(6.0)
        .fill_width(),
        column((
            button("Import JSON")
                .padding((10.0, 16.0))
                .fill_width()
                .on_click(move || import_open.set(true)),
            button(move || {
                let is_dark = theme().background.r < 0.3;
                if is_dark {
                    "Light Mode".to_string()
                } else {
                    "Dark Mode".to_string()
                }
            })
            .secondary()
            .padding((9.0, 14.0))
            .fill_width()
            .on_click(move || {
                let is_dark = theme().background.r < 0.3;
                let next = !is_dark;
                dark.set(next);
                let base = if next {
                    Theme::PLAYFUL_DARK
                } else {
                    Theme::PLAYFUL_LIGHT
                };
                set_theme(base.with_accent(theme().accent));
            }),
        ))
        .gap(10.0)
        .padding((18.0, 0.0))
        .fill_width(),
        text(move || status_msg.get())
            .size(11.5)
            .color(|| theme().ink_soft),
    ))
    .gap(12.0)
    .padding((12.0, 16.0))
    .width(216.0)
    .height(740.0)
    .align(Align::Start)
    .background(|| theme().surface)
    .border(1.0, || theme().border);

    // ---- Overview Panel ----
    let bar_hits = Rc::new(RefCell::new(HitRegions::<usize>::new()));
    let bar_hits_draw = bar_hits.clone();
    let bar_hits_hover = bar_hits;

    let spending_chart = column((
        row((
            text("Spending by Category")
                .size(15.0)
                .weight(FontWeight::BOLD)
                .color(|| theme().ink),
            text(move || {
                hovered_bar.get().map_or_else(
                    || "Hover a bar".to_string(),
                    |i| format!("{}: ${:.0}", CATEGORIES[i], bar_vals.get()[i]),
                )
            })
            .size(13.0)
            .weight(FontWeight::SEMIBOLD)
            .color(|| theme().accent),
        ))
        .fill_width()
        .justify(Justify::SpaceBetween),
        canvas(move |p, rect| {
            let t = theme();
            let vals = bar_vals.get();
            let hov = hovered_bar.get();
            let mut reg = bar_hits_draw.borrow_mut();
            reg.clear();

            let max_v = vals.iter().copied().fold(100.0_f64, f64::max);
            let pad_l = 12.0;
            let pad_r = 12.0;
            let pad_t = 14.0;
            let pad_b = 26.0;
            let plot_w = (rect.width() - pad_l - pad_r).max(1.0);
            let plot_h = (rect.height() - pad_t - pad_b).max(1.0);

            for step in 0..=3 {
                let y = rect.y0 + pad_t + plot_h * (step as f64 / 3.0);
                p.stroke_line(
                    Point::new(rect.x0 + pad_l, y),
                    Point::new(rect.x0 + pad_l + plot_w, y),
                    1.0,
                    t.border.with_alpha(0.55),
                );
            }

            let slot_w = plot_w / 5.0;
            let bar_w = (slot_w * 0.54).max(18.0);
            let lbl_style = TextStyle::new(12.0)
                .family(FontFamily::Ui)
                .color(t.ink_soft);
            for (i, &v) in vals.iter().enumerate() {
                let frac = (v / max_v).clamp(0.06, 1.0);
                let bh = plot_h * frac;
                let lx = pad_l + slot_w * (i as f64) + (slot_w - bar_w) * 0.5;
                let ly = pad_t + plot_h - bh;
                reg.push_rect(i, Rect::from_xywh(lx, ly, bar_w, bh));

                let sr = Rect::from_xywh(rect.x0 + lx, rect.y0 + ly, bar_w, bh);
                let base_c = category_color(CATEGORIES[i]);
                let top_c = if hov == Some(i) {
                    base_c.mix(Color::WHITE, 0.24)
                } else {
                    base_c
                };
                p.fill_rect(
                    sr,
                    10.0,
                    Brush::linear(
                        Point::new(sr.x0, sr.y0),
                        Point::new(sr.x0, sr.y1),
                        [(0.0, top_c), (1.0, base_c.with_alpha(0.62))],
                    ),
                );
                let mw = p.measure_text(CATEGORIES[i], None, &lbl_style).width;
                p.text(
                    CATEGORIES[i],
                    Point::new(sr.center().x - mw * 0.5, rect.y0 + pad_t + plot_h + 7.0),
                    &lbl_style,
                );
            }
        })
        .width(400.0)
        .height(190.0)
        .on_hover(move |pt| {
            let hit = pt.and_then(|p| bar_hits_hover.borrow().hit_test(p).copied());
            hovered_bar.set_if_changed(hit);
        }),
    ))
    .gap(10.0)
    .padding(18.0)
    .background(|| theme().surface)
    .border(1.0, || theme().border)
    .radius(22.0)
    .elevation(4.0)
    .shadow_tint(|| theme().accent);

    let cashflow_chart = column((
        row((
            text("6-Month Cashflow")
                .size(15.0)
                .weight(FontWeight::BOLD)
                .color(|| theme().ink),
            text("Net positive")
                .size(13.0)
                .weight(FontWeight::SEMIBOLD)
                .color(|| theme().positive),
        ))
        .fill_width()
        .justify(Justify::SpaceBetween),
        canvas(move |p, rect| {
            let t = theme();
            let mut monthly = [0.0f64; 6];
            transactions.with(|all| {
                for tx in all {
                    if let Some(m) = tx
                        .date
                        .split('-')
                        .nth(1)
                        .and_then(|s| s.parse::<usize>().ok())
                        && (1..=6).contains(&m)
                    {
                        monthly[m - 1] += tx.amount_cents as f64 / 100.0;
                    }
                }
            });
            let min_v = monthly.iter().copied().fold(0.0_f64, f64::min);
            let max_v = monthly.iter().copied().fold(1000.0_f64, f64::max);
            let span = (max_v - min_v).max(1.0);

            let pad_l = 16.0;
            let pad_r = 16.0;
            let pad_t = 16.0;
            let pad_b = 26.0;
            let plot_w = (rect.width() - pad_l - pad_r).max(1.0);
            let plot_h = (rect.height() - pad_t - pad_b).max(1.0);
            let base_y = rect.y0 + pad_t + plot_h;

            for step in 0..=3 {
                let y = rect.y0 + pad_t + plot_h * (step as f64 / 3.0);
                p.stroke_line(
                    Point::new(rect.x0 + pad_l, y),
                    Point::new(rect.x0 + pad_l + plot_w, y),
                    1.0,
                    t.border.with_alpha(0.55),
                );
            }

            let mut pts = [Point::ZERO; 6];
            for (i, &v) in monthly.iter().enumerate() {
                let x = rect.x0 + pad_l + plot_w * (i as f64 / 5.0);
                let norm = ((v - min_v) / span).clamp(0.08, 0.95);
                let y = rect.y0 + pad_t + plot_h * (1.0 - norm);
                pts[i] = Point::new(x, y);
            }

            let mut area = Path::new();
            area.move_to(Point::new(pts[0].x, base_y));
            area.line_to(pts[0]);
            for &pt in &pts[1..] {
                area.line_to(pt);
            }
            area.line_to(Point::new(pts[5].x, base_y));
            area.close();
            p.fill_path(
                &area,
                Brush::linear(
                    Point::new(rect.x0, rect.y0 + pad_t),
                    Point::new(rect.x0, base_y),
                    [
                        (0.0, t.positive.with_alpha(0.32)),
                        (1.0, t.positive.with_alpha(0.02)),
                    ],
                ),
            );

            let mut line = Path::new();
            line.move_to(pts[0]);
            for &pt in &pts[1..] {
                line.line_to(pt);
            }
            p.stroke_path(&line, 3.0, t.positive);

            const MONTHS: [&str; 6] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun"];
            let lbl_style = TextStyle::new(12.0)
                .family(FontFamily::Ui)
                .color(t.ink_soft);
            for (i, &pt) in pts.iter().enumerate() {
                p.fill_circle(pt, 6.0, t.surface);
                p.fill_circle(pt, 4.5, t.positive);
                let mw = p.measure_text(MONTHS[i], None, &lbl_style).width;
                p.text(
                    MONTHS[i],
                    Point::new(pt.x - mw * 0.5, base_y + 7.0),
                    &lbl_style,
                );
            }
        })
        .width(340.0)
        .height(190.0),
    ))
    .gap(10.0)
    .padding(18.0)
    .background(|| theme().surface)
    .border(1.0, || theme().border)
    .radius(22.0)
    .elevation(4.0)
    .shadow_tint(|| theme().positive);

    let overview_panel = column((
        grid(
            3,
            (
                kpi_card(
                    "Net Balance",
                    move || {
                        let sum: i64 =
                            transactions.with(|all| all.iter().map(|t| t.amount_cents).sum());
                        format_dollars(sum)
                    },
                    "All accounts · local JSON",
                    || theme().accent,
                    IconName::Wallet,
                ),
                kpi_card(
                    "Total Income",
                    move || {
                        let sum: i64 = transactions.with(|all| {
                            all.iter()
                                .filter(|t| t.amount_cents > 0)
                                .map(|t| t.amount_cents)
                                .sum()
                        });
                        format_dollars(sum)
                    },
                    "Payroll & dividends",
                    || theme().positive,
                    IconName::TrendingUp,
                ),
                kpi_card(
                    "Total Spending",
                    move || {
                        let sum: i64 = transactions.with(|all| {
                            all.iter()
                                .filter(|t| t.amount_cents < 0)
                                .map(|t| -t.amount_cents)
                                .sum()
                        });
                        format_dollars(sum)
                    },
                    "Across 5 budget categories",
                    || theme().negative,
                    IconName::CreditCard,
                ),
            ),
        )
        .gap(14.0)
        .fill_width(),
        row((spending_chart, cashflow_chart)).gap(14.0),
        column((
            row((
                text("Recent Activity")
                    .size(15.0)
                    .weight(FontWeight::BOLD)
                    .color(|| theme().ink),
                button("View All Transactions →")
                    .secondary()
                    .padding((6.0, 12.0))
                    .on_click(move || section.set(NavSection::Transactions)),
            ))
            .fill_width()
            .justify(Justify::SpaceBetween),
            table(transactions, tx_columns())
                .row_height(34.0)
                .header_height(36.0)
                .width(790.0)
                .height(210.0),
        ))
        .gap(10.0)
        .fill_width(),
    ))
    .gap(16.0)
    .fill_width()
    .visible(move || section.get() == NavSection::Overview);

    // ---- Transactions Panel ----
    let transactions_panel = column((
        row((
            text_input(search)
                .placeholder("Search merchant or category…")
                .width(340.0),
            select(
                cat_filter,
                vec![
                    "All Categories".into(),
                    "Groceries".into(),
                    "Dining".into(),
                    "Tech".into(),
                    "Travel".into(),
                    "Health".into(),
                    "Income".into(),
                ],
            )
            .width(190.0),
            text(move || format!("{} rows", filtered_tx.with(Vec::len)))
                .size(13.0)
                .weight(FontWeight::SEMIBOLD)
                .color(|| theme().ink_soft),
        ))
        .gap(12.0)
        .fill_width(),
        table(filtered_tx, tx_columns())
            .row_height(36.0)
            .header_height(38.0)
            .selection_mode(SelectionMode::Multiple)
            .selected(selected)
            .sort(sort)
            .width(790.0)
            .height(540.0),
    ))
    .gap(14.0)
    .fill_width()
    .visible(move || section.get() == NavSection::Transactions);

    // ---- Budgets & Add Transaction Panel ----
    let mut budget_rows: Vec<AnyView> = Vec::new();
    for idx in 0..5 {
        let cat_name = CATEGORIES[idx];
        let limit_sig =
            signal(budgets.with_untracked(|b| b.get(idx).map_or(500.0, |x| x.limit_dollars)));
        effect(move || {
            let v = limit_sig.get();
            budgets.update(|b| {
                if let Some(slot) = b.get_mut(idx) {
                    slot.limit_dollars = v;
                }
            });
        });

        let spent = move || {
            transactions.with(|all| {
                let total_cents: i64 = all
                    .iter()
                    .filter(|t| t.category == cat_name && t.amount_cents < 0)
                    .map(|t| -t.amount_cents)
                    .sum();
                // Monthly average across 6 months
                (total_cents as f64 / 100.0) / 6.0
            })
        };

        let row_view = column((
            row((
                text(cat_name)
                    .size(15.0)
                    .weight(FontWeight::BOLD)
                    .color(category_color(cat_name)),
                text(move || format!("${:.0} / ${:.0} mo", spent(), limit_sig.get()))
                    .size(13.0)
                    .weight(FontWeight::SEMIBOLD)
                    .color(|| theme().ink_soft),
            ))
            .fill_width()
            .justify(Justify::SpaceBetween),
            row((column(())
                .height(10.0)
                .radius(5.0)
                .width(move || {
                    let ratio = (spent() / limit_sig.get().max(1.0)).clamp(0.04, 1.0);
                    340.0 * ratio
                })
                .background(move || {
                    let ratio = spent() / limit_sig.get().max(1.0);
                    if ratio > 0.95 {
                        theme().negative
                    } else if ratio > 0.75 {
                        theme().warning
                    } else {
                        theme().positive
                    }
                }),))
            .width(340.0)
            .height(10.0)
            .radius(5.0)
            .background(|| theme().ink.with_alpha(0.08))
            .justify(Justify::Start),
            slider(limit_sig, 100.0, 2000.0)
                .step(25.0)
                .width(340.0)
                .a11y_label(format!("{cat_name} budget")),
        ))
        .gap(8.0)
        .align(Align::Start)
        .padding(14.0)
        .background(|| theme().surface)
        .border(1.0, || theme().border)
        .radius(18.0)
        .into_any();
        budget_rows.push(row_view);
    }

    let add_tx_form = column((
        text("Add Transaction")
            .size(16.0)
            .weight(FontWeight::BOLD)
            .color(|| theme().ink),
        text_input(new_merchant)
            .placeholder("Merchant name (e.g. Philz Coffee)")
            .width(320.0),
        select(
            new_cat_idx,
            CATEGORIES.iter().map(|s| (*s).to_string()).collect(),
        )
        .width(220.0),
        row((
            slider(new_amount, 5.0, 500.0)
                .step(5.0)
                .width(220.0)
                .a11y_label("Amount"),
            text(move || format!("${:.0}", new_amount.get()))
                .size(15.0)
                .weight(FontWeight::BOLD)
                .color(|| theme().accent),
        ))
        .gap(12.0),
        button("Add & Save to JSON")
            .padding((10.0, 20.0))
            .on_click(move || {
                let merchant = new_merchant.get_untracked().trim().to_string();
                let merchant = if merchant.is_empty() {
                    "Local Cafe".to_string()
                } else {
                    merchant
                };
                let cat = CATEGORIES[new_cat_idx.get_untracked().min(5)].to_string();
                let cents = (new_amount.get_untracked() * 100.0).round() as i64;
                let signed = if cat == "Income" { cents } else { -cents };
                transactions.update(|all| {
                    let next_id = all.iter().map(|t| t.id).max().unwrap_or(0) + 1;
                    all.insert(
                        0,
                        Transaction {
                            id: next_id,
                            merchant,
                            category: cat,
                            date: "2026-06-28".into(),
                            amount_cents: signed,
                        },
                    );
                });
                transactions.with_untracked(|t| {
                    budgets.with_untracked(|b| save_store(t, b));
                });
                new_merchant.set(String::new());
                status_msg.set("Saved to local JSON".into());
            }),
    ))
    .gap(12.0)
    .align(Align::Start)
    .padding(20.0)
    .background(|| theme().surface)
    .border(1.0, || theme().border)
    .radius(22.0);

    let budgets_panel = row((
        scroll(column(budget_rows).gap(12.0).align(Align::Start))
            .width(380.0)
            .height(600.0),
        add_tx_form,
    ))
    .gap(18.0)
    .align(Align::Start)
    .visible(move || section.get() == NavSection::Budgets);

    // ---- Settings Panel ----
    let accent_btn = |name: &'static str, hex: u32| {
        button(name)
            .tint(Color::from_hex(hex))
            .padding((9.0, 16.0))
            .on_click(move || set_theme(theme().with_accent(Color::from_hex(hex))))
    };
    let settings_panel = column((
        text("Appearance & Data")
            .size(20.0)
            .weight(FontWeight::BOLD)
            .color(|| theme().ink),
        text("Pick an accent colour (every surface, glow and chart animates smoothly):")
            .size(14.0)
            .color(|| theme().ink_soft),
        row((
            accent_btn("Violet", 0x7B61FF),
            accent_btn("Coral", 0xFF5A5F),
            accent_btn("Mint", 0x00B894),
            accent_btn("Sky", 0x2F80ED),
            accent_btn("Amber", 0xFF9F1C),
        ))
        .gap(10.0),
        row((
            button("Reset Seed Data").secondary().on_click(move || {
                let seed = seed_store();
                transactions.set(seed.transactions.clone());
                budgets.set(seed.budgets.clone());
                save_store(&seed.transactions, &seed.budgets);
                status_msg.set("Reset to seed data".into());
            }),
            text("Press ⌘⌥I anytime to open the built-in rugkit inspector.")
                .size(13.0)
                .color(|| theme().ink_soft),
        ))
        .gap(14.0),
    ))
    .gap(16.0)
    .align(Align::Start)
    .padding(24.0)
    .background(|| theme().surface)
    .border(1.0, || theme().border)
    .radius(22.0)
    .visible(move || section.get() == NavSection::Settings);

    // ---- Main Area + Import Modal ----
    let top_bar = row((
        text(move || match section.get() {
            NavSection::Overview => "Overview".to_string(),
            NavSection::Transactions => "Transactions".to_string(),
            NavSection::Budgets => "Budgets & Add".to_string(),
            NavSection::Settings => "Settings".to_string(),
        })
        .size(24.0)
        .weight(FontWeight::BLACK)
        .color(|| theme().ink),
        text("rugkit flagship · local JSON")
            .size(13.0)
            .color(|| theme().ink_soft),
    ))
    .fill_width()
    .height(42.0)
    .justify(Justify::SpaceBetween)
    .draggable();

    let main_content = column((
        top_bar,
        overview_panel,
        transactions_panel,
        budgets_panel,
        settings_panel,
    ))
    .gap(14.0)
    .padding((22.0, 24.0))
    .width(844.0)
    .height(740.0)
    .align(Align::Start);

    let import_modal = modal(
        import_open,
        column((
            row((
                icon(IconName::Upload).size(22.0).color(|| theme().accent),
                text("Import Transactions (JSON)")
                    .size(20.0)
                    .weight(FontWeight::BOLD)
                    .color(|| theme().ink),
            ))
            .gap(10.0),
            text(
                "Ready to import 3 sample transactions (+$1,245.00 net) and persist to local JSON:",
            )
            .size(14.0)
            .color(|| theme().ink_soft),
            column((
                text("• Stripe Payout  ·  Income  ·  +$1,450.00").size(13.5),
                text("• Tartine Bakery ·  Dining  ·  -$42.50").size(13.5),
                text("• REI Co-op      ·  Travel  ·  -$162.50").size(13.5),
            ))
            .gap(6.0)
            .align(Align::Start)
            .padding(14.0)
            .background(|| theme().background)
            .radius(14.0),
            row((
                button("Cancel")
                    .secondary()
                    .on_click(move || import_open.set(false)),
                button("Import & Save").on_click(move || {
                    transactions.update(|all| {
                        let base_id = all.iter().map(|t| t.id).max().unwrap_or(0) + 1;
                        let batch = [
                            ("Stripe Payout", "Income", 145_000),
                            ("Tartine Bakery", "Dining", -4_250),
                            ("REI Co-op", "Travel", -16_250),
                        ];
                        for (i, (m, c, amt)) in batch.into_iter().enumerate() {
                            all.insert(
                                0,
                                Transaction {
                                    id: base_id + i,
                                    merchant: m.into(),
                                    category: c.into(),
                                    date: "2026-06-29".into(),
                                    amount_cents: amt,
                                },
                            );
                        }
                    });
                    transactions.with_untracked(|t| {
                        budgets.with_untracked(|b| save_store(t, b));
                    });
                    status_msg.set("Imported 3 rows".into());
                    import_open.set(false);
                }),
            ))
            .gap(12.0),
        ))
        .gap(16.0)
        .align(Align::Start),
    );

    zstack((
        row((sidebar, main_content))
            .align(Align::Start)
            .fill_width(),
        import_modal,
    ))
    .fill_width()
}

fn tx_columns() -> Vec<Column<Transaction>> {
    vec![
        Column::text("id", "#", 64.0, |r: &Transaction| format!("{:04}", r.id))
            .sort_by(|a, b| a.id.cmp(&b.id)),
        Column::text("merchant", "Merchant", 230.0, |r: &Transaction| {
            r.merchant.clone()
        })
        .sort_by(|a, b| a.merchant.cmp(&b.merchant)),
        Column::custom(
            "category",
            "Category",
            136.0,
            |p, r: &Transaction, rect, _| {
                let c = category_color(&r.category);
                let pill =
                    Rect::from_xywh(rect.x0 + 8.0, rect.y0 + 6.0, 94.0, rect.height() - 12.0);
                p.fill_rect(pill, 999.0, c.with_alpha(0.16));
                let st = TextStyle::new(12.5)
                    .family(FontFamily::Ui)
                    .weight(FontWeight::SEMIBOLD)
                    .color(c);
                let w = p.measure_text(&r.category, None, &st).width;
                p.text(
                    &r.category,
                    Point::new(pill.center().x - w * 0.5, pill.y0 + 3.0),
                    &st,
                );
            },
        )
        .sort_by(|a, b| a.category.cmp(&b.category)),
        Column::text("date", "Date", 120.0, |r: &Transaction| r.date.clone())
            .sort_by(|a, b| a.date.cmp(&b.date)),
        Column::custom("amount", "Amount", 145.0, |p, r: &Transaction, rect, _| {
            let t = theme();
            let text_val = format_dollars(r.amount_cents);
            let st = TextStyle::new(13.5)
                .family(FontFamily::Ui)
                .weight(FontWeight::BOLD)
                .color(if r.amount_cents < 0 {
                    t.ink
                } else {
                    t.positive
                });
            let w = p.measure_text(&text_val, None, &st).width;
            let y = rect.y0 + ((rect.height() - 18.0) * 0.5).max(0.0);
            p.text(
                &text_val,
                Point::new((rect.x1 - w - 12.0).max(rect.x0 + 6.0), y),
                &st,
            );
        })
        .align(CellAlign::End)
        .sort_by(|a, b| a.amount_cents.cmp(&b.amount_cents)),
    ]
}

fn compute_category_totals(txs: &[Transaction]) -> [f64; 5] {
    let mut totals = [0.0f64; 5];
    for tx in txs {
        if tx.amount_cents < 0
            && let Some(idx) = CATEGORIES[..5].iter().position(|&c| c == tx.category)
        {
            totals[idx] += (-tx.amount_cents) as f64 / 100.0;
        }
    }
    totals
}

struct FinanceApp {
    ui: Ui,
    section: Signal<NavSection>,
    transactions: Signal<Vec<Transaction>>,
    import_open: Signal<bool>,
    bar_anim: [Animated<f64>; 5],
    bar_sig: Signal<[f64; 5]>,
    demo_mode: bool,
    demo_step: usize,
    next_demo: Duration,
    print_stats: bool,
    next_print: Duration,
    exit_after: Option<Duration>,
    forced_theme: Option<Theme>,
}

impl App for FinanceApp {
    fn attach(&mut self, wake: WakeHandle) {
        self.ui.attach(wake);
    }

    fn draw(&mut self, painter: &mut Painter<'_>, frame: &FrameInfo) {
        if let Some(t) = self.forced_theme {
            set_theme(t);
        }
        if self.demo_mode && frame.time >= self.next_demo {
            match self.demo_step {
                0 => self.section.set(NavSection::Transactions),
                1 => self.section.set(NavSection::Budgets),
                2 => {
                    self.section.set(NavSection::Overview);
                    self.import_open.set(true);
                }
                3 => self.import_open.set(false),
                _ => {}
            }
            self.demo_step = (self.demo_step + 1) % 4;
            self.next_demo = frame.time + Duration::from_millis(1600);
        }

        let targets = self
            .transactions
            .with_untracked(|t| compute_category_totals(t));
        let dt = frame.dt.as_secs_f64().min(1.0 / 20.0);
        let motion = Motion::Spring(Spring::BOUNCY);
        let mut moving = false;
        let mut vals = [0.0; 5];
        for i in 0..5 {
            self.bar_anim[i].set_target(targets[i]);
            moving |= self.bar_anim[i].step(dt, motion, frame.reduce_motion);
            vals[i] = self.bar_anim[i].value();
        }
        if moving {
            self.bar_sig.set(vals);
        }

        self.ui.draw(painter, frame);

        if self.print_stats && frame.time >= self.next_print {
            self.next_print = frame.time + Duration::from_secs(2);
            let s = frame.stats;
            eprintln!(
                "t={:>5.1}s fps={:>6.1} cpu={:.2}ms gpu={} worst={:.2}ms",
                frame.time.as_secs_f64(),
                s.fps,
                s.cpu_ms,
                s.gpu_ms.map_or("-".into(), |g| format!("{g:.2}ms")),
                s.worst_ms
            );
        }
        if let Some(limit) = self.exit_after
            && frame.time >= limit
        {
            std::process::exit(0);
        }
    }

    fn event(&mut self, event: &InputEvent) {
        if self.forced_theme.is_some() && matches!(event, InputEvent::AppearanceChanged(_)) {
            return;
        }
        self.ui.event(event);
    }

    fn wants_frame(&self) -> bool {
        self.demo_mode
            || self.exit_after.is_some()
            || self.bar_anim.iter().any(|a| !a.is_settled())
            || self.ui.wants_frame()
    }

    fn ime_state(&self) -> Option<ImeState> {
        self.ui.ime_state()
    }

    fn a11y_tree(&mut self) -> Option<A11yTree> {
        self.ui.a11y_tree()
    }

    fn a11y_action(&mut self, action: &A11yAction) {
        self.ui.a11y_action(action);
    }

    fn take_commands(&mut self) -> Vec<WindowCommand> {
        self.ui.take_commands()
    }

    fn take_new_windows(&mut self) -> Vec<WindowSpec> {
        self.ui.take_new_windows()
    }
}

fn main() -> Result<(), rugkit::Error> {
    let forced_theme = match std::env::var("FINANCE_THEME").ok().as_deref() {
        Some("light") => Some(Theme::PLAYFUL_LIGHT),
        Some("dark") => Some(Theme::PLAYFUL_DARK),
        _ => None,
    };
    if let Some(t) = forced_theme {
        set_theme(t);
    }

    let store = load_store();
    let initial_bars = compute_category_totals(&store.transactions);
    let transactions = signal(store.transactions);
    let budgets = signal(store.budgets);
    let section = signal(NavSection::Overview);
    let import_open = signal(false);
    let bar_sig = signal(initial_bars);
    let dark = signal(forced_theme.is_some_and(|t| t == Theme::PLAYFUL_DARK));
    let status_msg = signal(String::from("Local JSON synced"));

    let ui = Ui::new(move || {
        finance_view(
            section,
            transactions,
            budgets,
            import_open,
            bar_sig,
            dark,
            status_msg,
        )
    })
    .inspector_enabled(true);

    let app = FinanceApp {
        ui,
        section,
        transactions,
        import_open,
        bar_anim: initial_bars.map(Animated::new),
        bar_sig,
        demo_mode: std::env::var_os("FINANCE_DEMO").is_some(),
        demo_step: 0,
        next_demo: Duration::from_millis(1600),
        print_stats: std::env::var_os("FINANCE_PRINT_STATS").is_some(),
        next_print: Duration::from_secs(2),
        exit_after: std::env::var("FINANCE_EXIT_AFTER")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .map(Duration::from_secs_f64),
        forced_theme,
    };

    rugkit::run(
        WindowOptions {
            title: "Minty · rugkit personal finance".into(),
            size: Size::new(1060.0, 740.0),
            background: theme().background,
            measure_gpu: std::env::var_os("FINANCE_PRINT_STATS").is_some(),
            transparent_titlebar: true,
            menu_bar: Some(MenuBar::standard_app("Minty")),
        },
        app,
    )
}
