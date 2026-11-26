//! M3 Widget Gallery: showcases every rugkit widget in light and dark mode, subtree theme
//! overrides, RTL mirroring, Lucide icons, virtualised 100k-item list, modal/popover/tooltip,
//! multi-window shared signals and the `⌘⌥I` in-app inspector.
//!
//! ```bash
//! cargo run --release --example gallery
//! ```
//!
//! Environment variables:
//! - `M3_THEME=light` or `M3_THEME=dark` forces the initial theme (useful for screenshots).
//! - `M3_INSPECTOR=1` opens the in-app inspector on startup.
//! - `M3_EXIT_AFTER=<seconds>` exits after that many seconds.

use std::time::Duration;

use rugkit::prelude::*;
use rugkit::{A11yAction, A11yTree, ImeState, InputEvent, Ui, WakeHandle, WindowCommand};

fn card(title: &'static str, body: impl View) -> impl View {
    column((
        text(title)
            .size(16.0)
            .weight(FontWeight::BOLD)
            .color(|| theme().ink),
        body,
    ))
    .gap(14.0)
    .align(Align::Start)
    .padding(22.0)
    .background(|| theme().surface)
    .border(1.0, || theme().border)
    .radius(24.0)
    .elevation(5.0)
    .shadow_tint(|| theme().accent)
}

fn gradient_badge() -> ImageData {
    let (w, h) = (72u32, 48u32);
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let u = x as f32 / w as f32;
            let v = y as f32 / h as f32;
            let r = ((0.48 + 0.45 * u) * 255.0) as u8;
            let g = ((0.35 + 0.45 * v) * 255.0) as u8;
            let b = ((0.95 - 0.35 * u) * 255.0) as u8;
            px.extend_from_slice(&[r, g, b, 255]);
        }
    }
    ImageData::from_rgba(w, h, px)
}

fn gallery_view(dark: Signal<bool>, shared_count: Signal<i32>) -> impl View {
    let cb_on = signal(true);
    let cb_off = signal(false);
    let sw_on = signal(true);
    let sw_off = signal(false);
    let radio_val = signal("Playful");
    let slider_val = signal(64.0);
    let text_val = signal(String::from("Hello rugkit!"));
    let disabled_val = signal(String::from("Read-only field"));
    let dropdown = signal(0usize);
    let active_tab = signal(0usize);
    let modal_open = signal(false);
    let pop_open = signal(false);

    let header = row((
        row((
            icon(IconName::Layers).size(22.0).color(|| theme().accent),
            text("rugkit gallery")
                .size(20.0)
                .weight(FontWeight::BLACK)
                .color(|| theme().ink),
        ))
        .gap(10.0),
        row((
            button(move || {
                if dark.get() {
                    "Light Mode".to_string()
                } else {
                    "Dark Mode".to_string()
                }
            })
            .secondary()
            .padding((8.0, 16.0))
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
            button(move || format!("Shared: {}", shared_count.get()))
                .padding((8.0, 16.0))
                .on_click(move || shared_count.update(|n| *n += 1)),
        ))
        .gap(10.0),
    ))
    .fill_width()
    .height(48.0)
    .padding((6.0, 24.0))
    .justify(Justify::SpaceBetween)
    .draggable();

    let buttons_card = card(
        "Buttons & Icons",
        column((
            row((
                button("Primary").on_click(move || shared_count.update(|n| *n += 1)),
                button("Secondary").secondary(),
                button("Coral").tint(Color::from_hex(0xFF5A5F)),
                button("Disabled").disabled(true),
            ))
            .gap(10.0),
            row((
                tooltip(
                    "Heart",
                    icon(IconName::Heart).color(Color::from_hex(0xFF5A5F)),
                ),
                tooltip(
                    "Star",
                    icon(IconName::Star).color(Color::from_hex(0xFF9F1C)),
                ),
                tooltip("Wallet", icon(IconName::Wallet).color(|| theme().accent)),
                tooltip(
                    "Trending",
                    icon(IconName::TrendingUp).color(|| theme().positive),
                ),
                tooltip("Settings", icon(IconName::Settings)),
                tooltip("Notifications", icon(IconName::Bell)),
            ))
            .gap(14.0),
        ))
        .gap(16.0)
        .align(Align::Start),
    );

    let controls_card = card(
        "Controls & Slider",
        column((
            row((
                checkbox(cb_on).label("Checked"),
                checkbox(cb_off).label("Unchecked"),
                checkbox(signal(true)).label("Disabled").disabled(true),
            ))
            .gap(16.0),
            row((
                toggle(sw_on).label("Springs"),
                toggle(sw_off).label("Muted"),
                toggle(signal(false)).label("Locked").disabled(true),
            ))
            .gap(16.0),
            row((
                radio(radio_val, "Playful").label("Playful"),
                radio(radio_val, "Snappy").label("Snappy"),
                radio(radio_val, "Gentle").label("Gentle"),
            ))
            .gap(16.0),
            row((
                slider(slider_val, 0.0, 100.0)
                    .step(1.0)
                    .width(240.0)
                    .a11y_label("Intensity"),
                text(move || format!("{:.0}%", slider_val.get()))
                    .size(15.0)
                    .weight(FontWeight::BOLD)
                    .color(|| theme().accent),
            ))
            .gap(12.0),
        ))
        .gap(14.0)
        .align(Align::Start),
    );

    let inputs_card = card(
        "TextInput & Select",
        column((
            text_input(text_val)
                .placeholder("Type or use CJK IME…")
                .width(340.0),
            text_input(disabled_val).disabled(true).width(340.0),
            select(
                dropdown,
                vec![
                    "Personal Finance".into(),
                    "Design System".into(),
                    "Analytics".into(),
                ],
            )
            .width(220.0),
        ))
        .gap(12.0)
        .align(Align::Start),
    );

    let overlays_card = card(
        "Tabs, Popover & Modal",
        column((
            tabs(
                active_tab,
                vec![
                    (
                        "Overview".into(),
                        text("Implicit springs keep velocity on every state transition.")
                            .size(14.0)
                            .color(|| theme().ink_soft)
                            .into_any(),
                    ),
                    (
                        "Accessibility".into(),
                        text("Every widget exports role, label, checked and numeric state.")
                            .size(14.0)
                            .color(|| theme().ink_soft)
                            .into_any(),
                    ),
                    (
                        "Inspector".into(),
                        text("Press ⌘⌥I to inspect bounds, slow-mo and live theme tokens.")
                            .size(14.0)
                            .color(|| theme().ink_soft)
                            .into_any(),
                    ),
                ],
            ),
            row((
                popover(
                    pop_open,
                    button("Toggle Popover")
                        .secondary()
                        .padding((10.0, 18.0))
                        .on_click(move || pop_open.update(|o| *o = !*o)),
                    text("Anchored popover card").size(14.0),
                ),
                button("Open Modal")
                    .padding((10.0, 18.0))
                    .on_click(move || modal_open.set(true)),
            ))
            .gap(12.0),
        ))
        .gap(14.0)
        .align(Align::Start),
    );

    let scoped_card = theme_scope(
        |t| t.with_accent(Color::from_hex(0xFF5A5F)),
        card(
            "Subtree Theme & RTL",
            column((
                row((
                    button("Coral Accent").padding((10.0, 18.0)),
                    image(gradient_badge()).radius(12.0),
                ))
                .gap(14.0),
                direction_scope(
                    Direction::Rtl,
                    row((
                        text("RTL mirrored →").size(14.0).color(|| theme().ink_soft),
                        button("First").secondary().padding((6.0, 14.0)),
                        button("Second").padding((6.0, 14.0)),
                    ))
                    .gap(10.0),
                ),
            ))
            .gap(14.0)
            .align(Align::Start),
        ),
    );

    let list_card = card(
        "Virtualised List (100,000 rows)",
        virtual_list(100_000usize, 32.0, |p, i, r| {
            let t = theme();
            if i % 2 == 0 {
                p.fill_rect(r, 8.0, t.ink.with_alpha(0.04));
            }
            p.text(
                &format!("Transaction #{:06}", i + 1),
                Point::new(r.x0 + 12.0, r.y0 + 7.0),
                &TextStyle::new(14.0).family(FontFamily::Ui).color(t.ink),
            );
            p.text(
                &format!("${:.2}", ((i * 37) % 950) as f64 + 12.50),
                Point::new(r.x1 - 88.0, r.y0 + 7.0),
                &TextStyle::new(14.0)
                    .family(FontFamily::Ui)
                    .weight(FontWeight::SEMIBOLD)
                    .color(t.positive),
            );
        })
        .width(380.0)
        .height(144.0)
        .border(1.0, || theme().border)
        .radius(14.0),
    );

    let body = scroll(
        grid(
            2,
            (
                buttons_card,
                controls_card,
                inputs_card,
                overlays_card,
                scoped_card,
                list_card,
            ),
        )
        .gap(18.0)
        .padding((8.0, 24.0))
        .fill_width(),
    )
    .fill_width()
    .height(700.0);

    let page = column((header, body))
        .gap(8.0)
        .padding((24.0, 0.0))
        .fill_width();

    zstack((
        page,
        modal(
            modal_open,
            column((
                text("Modal Dialog").size(22.0).weight(FontWeight::BOLD),
                text("Click outside, press Escape, or click Close to dismiss.")
                    .size(15.0)
                    .color(|| theme().ink_soft),
                button("Close").on_click(move || modal_open.set(false)),
            ))
            .gap(16.0),
        ),
    ))
    .fill_width()
}

struct GalleryApp {
    ui: Ui,
    exit_after: Option<Duration>,
    forced_theme: Option<Theme>,
}

impl App for GalleryApp {
    fn attach(&mut self, wake: WakeHandle) {
        self.ui.attach(wake);
    }

    fn draw(&mut self, painter: &mut Painter<'_>, frame: &FrameInfo) {
        if let Some(t) = self.forced_theme {
            set_theme(t);
        }
        self.ui.draw(painter, frame);
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
        self.exit_after.is_some() || self.ui.wants_frame()
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
    let forced_theme = match std::env::var("M3_THEME").ok().as_deref() {
        Some("light") => Some(Theme::PLAYFUL_LIGHT),
        Some("dark") => Some(Theme::PLAYFUL_DARK),
        _ => None,
    };
    if let Some(t) = forced_theme {
        set_theme(t);
    }
    let dark = signal(forced_theme.is_some_and(|t| t == Theme::PLAYFUL_DARK));
    let shared_count = signal(0);

    let mut ui = Ui::new(move || gallery_view(dark, shared_count)).inspector_enabled(true);
    if std::env::var_os("M3_INSPECTOR").is_some() {
        ui.toggle_inspector();
    }

    let app = GalleryApp {
        ui,
        exit_after: std::env::var("M3_EXIT_AFTER")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .map(Duration::from_secs_f64),
        forced_theme,
    };

    rugkit::run(
        WindowOptions {
            title: "rugkit · widget gallery".into(),
            size: Size::new(1040.0, 780.0),
            background: theme().background,
            transparent_titlebar: true,
            menu_bar: Some(MenuBar::standard_app("rugkit")),
            ..WindowOptions::default()
        },
        app,
    )
}
