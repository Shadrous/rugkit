//! M4 demo: reactive animated Canvas charts (bar & trend line with hit-testing) paired with a
//! 100,000-row virtualised Table (sticky header, sort, resize, multi-row selection, custom cells).
//!
//! ```bash
//! cargo run --release --example canvas_chart
//! ```
//!
//! Environment variables:
//! - `M4_PRINT_STATS=1` prints frame stats to stderr every 2 seconds.
//! - `M4_AUTOPLAY=1` cycles datasets and scrolls the 100k-row table continuously for perf testing.
//! - `M4_THEME=light` or `M4_THEME=dark` forces the initial theme.
//! - `M4_EXIT_AFTER=<seconds>` exits after that many seconds.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::time::Duration;

use rugkit::prelude::*;
use rugkit::{
    A11yAction, A11yTree, Animated, ImeState, InputEvent, Motion, Ui, WakeHandle, WindowCommand,
};

const MONTHS: [&str; 6] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun"];
const DATASETS: [(&str, [f64; 6], [f64; 6]); 3] = [
    (
        "Q1 2026",
        [62.0, 48.0, 84.0, 56.0, 91.0, 74.0],
        [40.0, 52.0, 49.0, 68.0, 75.0, 88.0],
    ),
    (
        "Q2 2026",
        [88.0, 72.0, 46.0, 95.0, 60.0, 82.0],
        [62.0, 70.0, 58.0, 84.0, 79.0, 94.0],
    ),
    (
        "Q3 2026",
        [45.0, 92.0, 76.0, 64.0, 85.0, 98.0],
        [50.0, 66.0, 82.0, 74.0, 91.0, 96.0],
    ),
];

#[derive(Clone)]
struct Transaction {
    id: usize,
    merchant: &'static str,
    category: &'static str,
    date: String,
    amount_cents: i64,
}

fn generate_transactions(count: usize) -> Vec<Transaction> {
    const MERCHANTS: [(&str, &str, i64); 8] = [
        ("Blue Bottle Coffee", "Dining", -650),
        ("Whole Foods Market", "Groceries", -8420),
        ("Acme Payroll", "Income", 345_000),
        ("Apple Store", "Tech", -12_900),
        ("Metro Transit", "Transport", -475),
        ("Equinox Fitness", "Health", -21_000),
        ("Airbnb Stay", "Travel", -48_500),
        ("Vanguard Dividend", "Income", 42_500),
    ];
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let (merchant, category, base) = MERCHANTS[(i * 7 + (i / 13)) % MERCHANTS.len()];
        let jitter = ((i * 31) % 900) as i64;
        let amount_cents = if base >= 0 {
            base + jitter * 10
        } else {
            base - jitter
        };
        let month = (i % 12) + 1;
        let day = ((i * 5) % 28) + 1;
        out.push(Transaction {
            id: i + 1,
            merchant,
            category,
            date: format!("2026-{month:02}-{day:02}"),
            amount_cents,
        });
    }
    out
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

fn chart_view(
    dataset_idx: Signal<usize>,
    bars: Signal<[f64; 6]>,
    trend: Signal<[f64; 6]>,
    rows: Signal<Vec<Transaction>>,
    dark: Signal<bool>,
) -> impl View {
    let hovered_bar = signal(None::<usize>);
    let hovered_pt = signal(None::<usize>);
    let selected = signal(BTreeSet::new());
    let sort = signal(None::<(usize, SortDirection)>);

    let bar_hits = Rc::new(RefCell::new(HitRegions::<usize>::new()));
    let bar_hits_draw = bar_hits.clone();
    let bar_hits_hover = bar_hits;

    let pt_hits = Rc::new(RefCell::new(HitRegions::<usize>::new()));
    let pt_hits_draw = pt_hits.clone();
    let pt_hits_hover = pt_hits;

    let header = row((
        row((
            icon(IconName::TrendingUp)
                .size(22.0)
                .color(|| theme().accent),
            text("rugkit · canvas & 100k table")
                .size(19.0)
                .weight(FontWeight::BLACK)
                .color(|| theme().ink),
        ))
        .gap(10.0),
        row((
            button(DATASETS[0].0)
                .secondary()
                .padding((8.0, 14.0))
                .background(move || {
                    if dataset_idx.get() == 0 {
                        theme().accent
                    } else {
                        theme().surface
                    }
                })
                .label_color(move || {
                    if dataset_idx.get() == 0 {
                        Color::WHITE
                    } else {
                        theme().ink
                    }
                })
                .on_click(move || dataset_idx.set(0)),
            button(DATASETS[1].0)
                .secondary()
                .padding((8.0, 14.0))
                .background(move || {
                    if dataset_idx.get() == 1 {
                        theme().accent
                    } else {
                        theme().surface
                    }
                })
                .label_color(move || {
                    if dataset_idx.get() == 1 {
                        Color::WHITE
                    } else {
                        theme().ink
                    }
                })
                .on_click(move || dataset_idx.set(1)),
            button(DATASETS[2].0)
                .secondary()
                .padding((8.0, 14.0))
                .background(move || {
                    if dataset_idx.get() == 2 {
                        theme().accent
                    } else {
                        theme().surface
                    }
                })
                .label_color(move || {
                    if dataset_idx.get() == 2 {
                        Color::WHITE
                    } else {
                        theme().ink
                    }
                })
                .on_click(move || dataset_idx.set(2)),
            button(move || {
                if dark.get() {
                    "Light".to_string()
                } else {
                    "Dark".to_string()
                }
            })
            .secondary()
            .padding((8.0, 14.0))
            .on_click(move || {
                let next = !dark.get_untracked();
                dark.set(next);
                let base = if next {
                    Theme::PLAYFUL_DARK
                } else {
                    Theme::PLAYFUL_LIGHT
                };
                set_theme(base.with_accent(theme().accent));
            }),
        ))
        .gap(8.0),
    ))
    .fill_width()
    .height(46.0)
    .padding((4.0, 24.0))
    .justify(Justify::SpaceBetween)
    .draggable();

    // 1. Animated Bar Chart Canvas
    let bar_chart = column((
        row((
            text("Monthly Spending")
                .size(16.0)
                .weight(FontWeight::BOLD)
                .color(|| theme().ink),
            text(move || {
                hovered_bar.get().map_or_else(
                    || "Hover a bar".to_string(),
                    |i| format!("{}: ${:.0}", MONTHS[i], bars.get()[i] * 40.0),
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
            let vals = bars.get();
            let hov = hovered_bar.get();
            let mut reg = bar_hits_draw.borrow_mut();
            reg.clear();

            let pad_l = 12.0;
            let pad_r = 12.0;
            let pad_t = 14.0;
            let pad_b = 26.0;
            let plot_w = (rect.width() - pad_l - pad_r).max(1.0);
            let plot_h = (rect.height() - pad_t - pad_b).max(1.0);

            // Subtle horizontal grid lines.
            for step in 0..=3 {
                let y = rect.y0 + pad_t + plot_h * (step as f64 / 3.0);
                p.stroke_line(
                    Point::new(rect.x0 + pad_l, y),
                    Point::new(rect.x0 + pad_l + plot_w, y),
                    1.0,
                    t.border.with_alpha(0.55),
                );
            }

            let slot_w = plot_w / 6.0;
            let bar_w = (slot_w * 0.56).max(14.0);
            let lbl_style = TextStyle::new(12.0)
                .family(FontFamily::Ui)
                .color(t.ink_soft);

            for (i, &v) in vals.iter().enumerate() {
                let frac = (v / 100.0).clamp(0.04, 1.0);
                let bh = plot_h * frac;
                let lx = pad_l + slot_w * (i as f64) + (slot_w - bar_w) * 0.5;
                let ly = pad_t + plot_h - bh;
                let local_r = Rect::from_xywh(lx, ly, bar_w, bh);
                reg.push_rect(i, local_r);

                let screen_r = Rect::from_xywh(rect.x0 + lx, rect.y0 + ly, bar_w, bh);
                let is_hov = hov == Some(i);
                let top_c = if is_hov {
                    t.accent.mix(Color::WHITE, 0.22)
                } else {
                    t.accent
                };
                let bot_c = t.accent.with_alpha(if is_hov { 0.85 } else { 0.58 });
                p.fill_rect(
                    screen_r,
                    10.0,
                    Brush::linear(
                        Point::new(screen_r.x0, screen_r.y0),
                        Point::new(screen_r.x0, screen_r.y1),
                        [(0.0, top_c), (1.0, bot_c)],
                    ),
                );
                let mw = p.measure_text(MONTHS[i], None, &lbl_style).width;
                p.text(
                    MONTHS[i],
                    Point::new(
                        screen_r.center().x - mw * 0.5,
                        rect.y0 + pad_t + plot_h + 7.0,
                    ),
                    &lbl_style,
                );
            }
        })
        .width(440.0)
        .height(176.0)
        .on_hover(move |pt| {
            let hit = pt.and_then(|p| bar_hits_hover.borrow().hit_test(p).copied());
            hovered_bar.set_if_changed(hit);
        }),
    ))
    .gap(10.0)
    .padding(18.0)
    .background(|| theme().surface)
    .border(1.0, || theme().border)
    .radius(24.0)
    .elevation(5.0)
    .shadow_tint(|| theme().accent);

    // 2. Animated Trend Line & Area Chart Canvas
    let line_chart = column((
        row((
            text("Net Cashflow Trend")
                .size(16.0)
                .weight(FontWeight::BOLD)
                .color(|| theme().ink),
            text(move || {
                hovered_pt.get().map_or_else(
                    || "Hover a point".to_string(),
                    |i| format!("{}: +${:.0}", MONTHS[i], trend.get()[i] * 55.0),
                )
            })
            .size(13.0)
            .weight(FontWeight::SEMIBOLD)
            .color(|| theme().positive),
        ))
        .fill_width()
        .justify(Justify::SpaceBetween),
        canvas(move |p, rect| {
            let t = theme();
            let vals = trend.get();
            let hov = hovered_pt.get();
            let mut reg = pt_hits_draw.borrow_mut();
            reg.clear();

            let pad_l = 18.0;
            let pad_r = 18.0;
            let pad_t = 16.0;
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

            let mut pts = [Point::ZERO; 6];
            for (i, &v) in vals.iter().enumerate() {
                let lx = pad_l + plot_w * (i as f64 / 5.0);
                let ly = pad_t + plot_h * (1.0 - (v / 100.0).clamp(0.05, 0.98));
                reg.push_circle(i, Point::new(lx, ly), 14.0);
                pts[i] = Point::new(rect.x0 + lx, rect.y0 + ly);
            }

            // Area fill under the trend line.
            let mut area = Path::new();
            let base_y = rect.y0 + pad_t + plot_h;
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

            // Crisp line stroke.
            let mut line = Path::new();
            line.move_to(pts[0]);
            for &pt in &pts[1..] {
                line.line_to(pt);
            }
            p.stroke_path(&line, 3.0, t.positive);

            let lbl_style = TextStyle::new(12.0)
                .family(FontFamily::Ui)
                .color(t.ink_soft);
            for (i, &pt) in pts.iter().enumerate() {
                let is_hov = hov == Some(i);
                let r = if is_hov { 7.0 } else { 5.0 };
                p.fill_circle(pt, r + 2.0, t.surface);
                p.fill_circle(pt, r, t.positive);
                let mw = p.measure_text(MONTHS[i], None, &lbl_style).width;
                p.text(
                    MONTHS[i],
                    Point::new(pt.x - mw * 0.5, base_y + 7.0),
                    &lbl_style,
                );
            }
        })
        .width(440.0)
        .height(176.0)
        .on_hover(move |pt| {
            let hit = pt.and_then(|p| pt_hits_hover.borrow().hit_test(p).copied());
            hovered_pt.set_if_changed(hit);
        }),
    ))
    .gap(10.0)
    .padding(18.0)
    .background(|| theme().surface)
    .border(1.0, || theme().border)
    .radius(24.0)
    .elevation(5.0)
    .shadow_tint(|| theme().positive);

    // 3. 100,000-row Virtualised Transactions Table
    let columns = vec![
        Column::text("id", "#", 74.0, |r: &Transaction| format!("{:06}", r.id))
            .sort_by(|a, b| a.id.cmp(&b.id)),
        Column::text("merchant", "Merchant", 250.0, |r: &Transaction| {
            r.merchant.to_string()
        })
        .sort_by(|a, b| a.merchant.cmp(b.merchant)),
        Column::custom(
            "category",
            "Category",
            150.0,
            |p, r: &Transaction, rect, _| {
                let c = category_color(r.category);
                let pill =
                    Rect::from_xywh(rect.x0 + 10.0, rect.y0 + 7.0, 96.0, rect.height() - 14.0);
                p.fill_rect(pill, 999.0, c.with_alpha(0.16));
                let st = TextStyle::new(12.5)
                    .family(FontFamily::Ui)
                    .weight(FontWeight::SEMIBOLD)
                    .color(c);
                let w = p.measure_text(r.category, None, &st).width;
                p.text(
                    r.category,
                    Point::new(pill.center().x - w * 0.5, pill.y0 + 3.0),
                    &st,
                );
            },
        )
        .sort_by(|a, b| a.category.cmp(b.category)),
        Column::text("date", "Date", 140.0, |r: &Transaction| r.date.clone())
            .sort_by(|a, b| a.date.cmp(&b.date)),
        Column::custom("amount", "Amount", 160.0, |p, r: &Transaction, rect, _| {
            let t = theme();
            let neg = r.amount_cents < 0;
            let abs = r.amount_cents.abs();
            let text_val = if neg {
                format!("-${}.{:02}", abs / 100, abs % 100)
            } else {
                format!("+${}.{:02}", abs / 100, abs % 100)
            };
            let st = TextStyle::new(14.0)
                .family(FontFamily::Ui)
                .weight(FontWeight::BOLD)
                .color(if neg { t.ink } else { t.positive });
            let w = p.measure_text(&text_val, None, &st).width;
            let y = rect.y0 + ((rect.height() - 18.0) * 0.5).max(0.0);
            p.text(
                &text_val,
                Point::new((rect.x1 - w - 14.0).max(rect.x0 + 6.0), y),
                &st,
            );
        })
        .align(CellAlign::End)
        .sort_by(|a, b| a.amount_cents.cmp(&b.amount_cents)),
    ];

    let table_section = column((
        row((
            text("Transactions (100,000 rows · sortable · resizable · multi-select)")
                .size(15.0)
                .weight(FontWeight::BOLD)
                .color(|| theme().ink),
            text(move || {
                let n = selected.with(BTreeSet::len);
                if n == 0 {
                    "Click a row · ⌘-click multi · Shift-click range".to_string()
                } else {
                    format!("{n} selected")
                }
            })
            .size(13.0)
            .color(|| theme().ink_soft),
        ))
        .fill_width()
        .justify(Justify::SpaceBetween),
        table(rows, columns)
            .row_height(36.0)
            .header_height(40.0)
            .selection_mode(SelectionMode::Multiple)
            .selected(selected)
            .sort(sort)
            .width(976.0)
            .height(340.0),
    ))
    .gap(10.0)
    .padding((0.0, 24.0))
    .fill_width();

    column((
        header,
        row((bar_chart, line_chart)).gap(18.0).padding((0.0, 24.0)),
        table_section,
    ))
    .gap(16.0)
    .padding((22.0, 0.0))
    .fill_width()
}

struct ChartApp {
    ui: Ui,
    dataset_idx: Signal<usize>,
    bars_anim: [Animated<f64>; 6],
    trend_anim: [Animated<f64>; 6],
    bars_sig: Signal<[f64; 6]>,
    trend_sig: Signal<[f64; 6]>,
    last_dataset: usize,
    print_stats: bool,
    next_print: Duration,
    autoplay: bool,
    next_switch: Duration,
    exit_after: Option<Duration>,
    forced_theme: Option<Theme>,
}

impl App for ChartApp {
    fn attach(&mut self, wake: WakeHandle) {
        self.ui.attach(wake);
    }

    fn draw(&mut self, painter: &mut Painter<'_>, frame: &FrameInfo) {
        if let Some(t) = self.forced_theme {
            set_theme(t);
        }
        if self.autoplay && frame.time >= self.next_switch {
            self.dataset_idx.update(|i| *i = (*i + 1) % DATASETS.len());
            self.next_switch = frame.time + Duration::from_millis(1200);
        }
        if self.autoplay {
            // Scroll the 100k table continuously to stress-test 120fps scrolling.
            self.ui.event(&InputEvent::Scroll {
                delta: Vec2::new(0.0, -42.0),
                position: Point::new(500.0, 560.0),
            });
        }

        let cur = self.dataset_idx.get_untracked();
        if cur != self.last_dataset {
            self.last_dataset = cur;
            let (_, b_target, t_target) = DATASETS[cur];
            for i in 0..6 {
                self.bars_anim[i].set_target(b_target[i]);
                self.trend_anim[i].set_target(t_target[i]);
            }
        }
        let dt = frame.dt.as_secs_f64().min(1.0 / 20.0);
        let motion = Motion::Spring(Spring::BOUNCY);
        let mut moving = false;
        let mut b_vals = [0.0; 6];
        let mut t_vals = [0.0; 6];
        for i in 0..6 {
            moving |= self.bars_anim[i].step(dt, motion, frame.reduce_motion);
            moving |= self.trend_anim[i].step(dt, motion, frame.reduce_motion);
            b_vals[i] = self.bars_anim[i].value();
            t_vals[i] = self.trend_anim[i].value();
        }
        if moving {
            batch(|| {
                self.bars_sig.set(b_vals);
                self.trend_sig.set(t_vals);
            });
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
        let cur = self.dataset_idx.get_untracked();
        let chart_moving = cur != self.last_dataset
            || self.bars_anim.iter().any(|a| !a.is_settled())
            || self.trend_anim.iter().any(|a| !a.is_settled());
        self.autoplay || self.exit_after.is_some() || chart_moving || self.ui.wants_frame()
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
    let forced_theme = match std::env::var("M4_THEME").ok().as_deref() {
        Some("light") => Some(Theme::PLAYFUL_LIGHT),
        Some("dark") => Some(Theme::PLAYFUL_DARK),
        _ => None,
    };
    if let Some(t) = forced_theme {
        set_theme(t);
    }

    let dark = signal(forced_theme.is_some_and(|t| t == Theme::PLAYFUL_DARK));
    let dataset_idx = signal(0usize);
    let (_, b0, t0) = DATASETS[0];
    let bars_sig = signal(b0);
    let trend_sig = signal(t0);
    let rows = signal(generate_transactions(100_000));

    let ui = Ui::new(move || chart_view(dataset_idx, bars_sig, trend_sig, rows, dark))
        .inspector_enabled(true);

    let app = ChartApp {
        ui,
        dataset_idx,
        bars_anim: b0.map(Animated::new),
        trend_anim: t0.map(Animated::new),
        bars_sig,
        trend_sig,
        last_dataset: 0,
        print_stats: std::env::var_os("M4_PRINT_STATS").is_some(),
        next_print: Duration::from_secs(2),
        autoplay: std::env::var_os("M4_AUTOPLAY").is_some(),
        next_switch: Duration::from_millis(900),
        exit_after: std::env::var("M4_EXIT_AFTER")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .map(Duration::from_secs_f64),
        forced_theme,
    };

    rugkit::run(
        WindowOptions {
            title: "rugkit · canvas chart & 100k table".into(),
            size: Size::new(1024.0, 740.0),
            background: theme().background,
            measure_gpu: std::env::var_os("M4_PRINT_STATS").is_some(),
            transparent_titlebar: true,
            menu_bar: Some(MenuBar::standard_app("rugkit")),
        },
        app,
    )
}
