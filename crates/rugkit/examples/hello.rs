//! Minimal hello-world example used to verify cold-start latency (< 250ms) and release
//! binary size (< 8MB).
//!
//! ```bash
//! cargo run --release --example hello
//! ```

use std::time::Instant;

use rugkit::prelude::*;
use rugkit::{InputEvent, Ui, WakeHandle};

struct HelloApp {
    ui: Ui,
    started: Instant,
    bench_startup: bool,
    first_frame_logged: bool,
}

impl App for HelloApp {
    fn attach(&mut self, wake: WakeHandle) {
        self.ui.attach(wake);
    }

    fn draw(&mut self, painter: &mut Painter<'_>, frame: &FrameInfo) {
        self.ui.draw(painter, frame);
        if self.bench_startup && !self.first_frame_logged {
            self.first_frame_logged = true;
            eprintln!(
                "cold_start_ms={:.1}",
                self.started.elapsed().as_secs_f64() * 1000.0
            );
            std::process::exit(0);
        }
    }

    fn event(&mut self, event: &InputEvent) {
        self.ui.event(event);
    }

    fn wants_frame(&self) -> bool {
        self.ui.wants_frame()
    }
}

fn main() -> Result<(), rugkit::Error> {
    let started = Instant::now();
    let bench_startup = std::env::var_os("HELLO_BENCH_STARTUP").is_some();
    let ui = Ui::new(|| {
        column((
            text("Hello, rugkit!")
                .size(44.0)
                .weight(FontWeight::BLACK)
                .color(|| theme().accent),
            text("The one framework that cares about visuals.")
                .size(17.0)
                .color(|| theme().ink_soft),
        ))
        .gap(12.0)
    });
    rugkit::run(
        WindowOptions {
            title: "Hello rugkit".into(),
            size: Size::new(560.0, 320.0),
            background: theme().background,
            transparent_titlebar: true,
            ..WindowOptions::default()
        },
        HelloApp {
            ui,
            started,
            bench_startup,
            first_frame_logged: false,
        },
    )
}
