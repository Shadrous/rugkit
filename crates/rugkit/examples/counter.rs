//! Counter demo: signals, layout, implicit springs and M2 platform plumbing.
//!
//! Every state change springs (the number card resizes, the progress bar stretches, colours shift
//! with the sign, buttons lift on hover and squash when pressed). Supports:
//! - Tab / Shift+Tab focus rings and Enter / Space activation
//! - VoiceOver accessibility via AccessKit
//! - CJK IME composition and clipboard shortcuts in the note field
//! - Transparent macOS titlebar with draggable header and native menu bar (⌘= / ⌘- / ⌘0)
//! - System light/dark appearance and macOS accent colour, animated on change
//!
//! ```bash
//! cargo run --release --example counter
//! ```
//!
//! Environment variables:
//! - `M1_PRINT_STATS=1` prints frame stats to stderr every 2 seconds while frames are drawn.
//! - `M1_AUTOPLAY=1` changes the count on a timer (for screenshots and recordings).
//! - `M1_EXIT_AFTER=<seconds>` closes the demo after that long.

use std::time::Duration;

use rugkit::prelude::*;
use rugkit::{A11yAction, A11yTree, ImeState, InputEvent, Ui, WakeHandle, WindowCommand};

const MAX_STEPS: i32 = 10;
const TRACK: f64 = 320.0;

fn counter(count: Signal<i32>, note: Signal<String>) -> impl View {
    let tone = move || {
        let t = theme();
        match count.get() {
            0 => t.ink,
            c if c > 0 => t.accent,
            _ => t.negative,
        }
    };
    let caption = move || {
        match count.get() {
            0 => "Tap + or press Tab & Space",
            1..=3 => "Nice, keep going",
            4..=9 => "On a roll!",
            c if c >= 10 => "Maxed out 🎉",
            _ => "Below zero…",
        }
        .to_string()
    };

    column((
        // Draggable header under the transparent macOS titlebar.
        row((text("rugkit counter")
            .size(18.0)
            .weight(FontWeight::SEMIBOLD)
            .color(|| theme().ink_soft),))
        .fill_width()
        .height(36.0)
        .justify(Justify::Center)
        .draggable(),
        column((
            text(move || count.get().to_string())
                .size(112.0)
                .weight(FontWeight::BLACK)
                .letter_spacing(-3.0)
                .color(tone),
            text(caption).size(17.0).color(|| theme().ink_soft),
            // Progress track with a bar whose width (and colour) spring with the count.
            row((column(())
                .height(14.0)
                .radius(7.0)
                .width(move || {
                    let steps = count.get().unsigned_abs().min(MAX_STEPS as u32);
                    (TRACK * f64::from(steps) / f64::from(MAX_STEPS)).max(14.0)
                })
                .background(move || {
                    if count.get() == 0 {
                        theme().ink.with_alpha(0.12)
                    } else {
                        tone()
                    }
                }),))
            .width(TRACK)
            .height(14.0)
            .radius(7.0)
            .background(|| theme().ink.with_alpha(0.06))
            .justify(Justify::Start),
            row((
                button("−")
                    .secondary()
                    .a11y_label("Decrement")
                    .on_click(move || count.update(|c| *c = (*c - 1).max(-MAX_STEPS))),
                button("Reset").secondary().on_click(move || count.set(0)),
                button("+")
                    .a11y_label("Increment")
                    .on_click(move || count.update(|c| *c = (*c + 1).min(MAX_STEPS))),
            ))
            .gap(14.0),
            text_input(note)
                .placeholder("Add a note (try CJK IME or ⌘C / ⌘V)…")
                .width(TRACK),
        ))
        .gap(22.0)
        .padding((36.0, 48.0))
        .background(|| theme().surface)
        .radius(40.0)
        .elevation(move || if count.get() == 0 { 6.0 } else { 12.0 })
        .shadow_tint(|| theme().accent),
    ))
    .gap(20.0)
    .padding((12.0, 24.0))
}

/// Wraps the UI to print frame stats and drive the optional autoplay.
struct Demo {
    ui: Ui,
    count: Signal<i32>,
    print_stats: bool,
    next_print: Duration,
    autoplay: bool,
    next_step: Duration,
    step: usize,
    exit_after: Option<Duration>,
}

impl App for Demo {
    fn attach(&mut self, wake: WakeHandle) {
        self.ui.attach(wake);
    }

    fn draw(&mut self, p: &mut Painter<'_>, frame: &FrameInfo) {
        if self.autoplay && frame.time >= self.next_step {
            // Up to 6, back down to -3, reset: exercises growth, sign changes and interrupts.
            const SCRIPT: [i32; 12] = [1, 2, 3, 4, 5, 6, 3, 0, -1, -2, -3, 0];
            self.count.set(SCRIPT[self.step % SCRIPT.len()]);
            self.step += 1;
            self.next_step = frame.time + Duration::from_millis(700);
        }
        self.ui.draw(p, frame);
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
        self.ui.event(event);
    }

    fn wants_frame(&self) -> bool {
        self.autoplay || self.exit_after.is_some() || self.ui.wants_frame()
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
}

fn main() -> Result<(), rugkit::Error> {
    let count = signal(0);
    let note = signal(String::new());
    let ui = Ui::new(move || counter(count, note)).on_menu_action(move |id| match id {
        "counter.inc" => count.update(|c| *c = (*c + 1).min(MAX_STEPS)),
        "counter.dec" => count.update(|c| *c = (*c - 1).max(-MAX_STEPS)),
        "counter.reset" => count.set(0),
        _ => {}
    });

    let menu_bar = MenuBar::standard_app("rugkit").submenu(
        Submenu::new("Counter")
            .item(MenuItem::new("counter.inc", "Increment").shortcut(Shortcut::cmd("+")))
            .item(MenuItem::new("counter.dec", "Decrement").shortcut(Shortcut::cmd("-")))
            .separator()
            .item(MenuItem::new("counter.reset", "Reset").shortcut(Shortcut::cmd("0"))),
    );

    let demo = Demo {
        ui,
        count,
        print_stats: std::env::var_os("M1_PRINT_STATS").is_some(),
        next_print: Duration::from_secs(2),
        autoplay: std::env::var_os("M1_AUTOPLAY").is_some(),
        next_step: Duration::from_millis(800),
        step: 0,
        exit_after: std::env::var("M1_EXIT_AFTER")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .map(Duration::from_secs_f64),
    };
    rugkit::run(
        WindowOptions {
            title: "rugkit · counter".into(),
            size: Size::new(720.0, 680.0),
            background: theme().background,
            measure_gpu: std::env::var_os("M1_PRINT_STATS").is_some(),
            transparent_titlebar: true,
            menu_bar: Some(menu_bar),
        },
        demo,
    )
}
