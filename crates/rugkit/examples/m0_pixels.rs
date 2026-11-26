//! M0 demo: rounded shapes, layered soft shadows, gradients, crisp SF Pro Rounded text, a
//! moving element and live frame stats.
//!
//! ```bash
//! cargo run --release --example m0_pixels
//! ```
//!
//! Environment variables:
//! - `M0_PRINT_STATS=1` prints frame stats to stderr every 2 seconds.
//! - `M0_EXIT_AFTER=<seconds>` closes the demo after that long (handy for scripted checks).

use std::time::Duration;

use rugkit::prelude::*;

const INK: Color = Color::from_hex(0x1E1B3A);
const INK_SOFT: Color = Color::from_hex(0x5B5677);

struct Accent {
    from: Color,
    to: Color,
    title: &'static str,
    body: &'static str,
}

const ACCENTS: [Accent; 3] = [
    Accent {
        from: Color::from_hex(0xFF5A5F),
        to: Color::from_hex(0xFFA046),
        title: "Bold by default",
        body: "Saturated colour, generous radii and layered shadows. It looks finished before you tweak anything.",
    },
    Accent {
        from: Color::from_hex(0x7B61FF),
        to: Color::from_hex(0xD36CF0),
        title: "Motion everywhere",
        body: "From M1, every change animates with an interruptible spring that keeps its velocity.",
    },
    Accent {
        from: Color::from_hex(0x00BFA6),
        to: Color::from_hex(0x45DD8E),
        title: "Crisp text",
        body: "SF Pro Rounded for UI, SF Pro for dense content, shaped by parley and rendered on the GPU by vello.",
    },
];

#[derive(Default)]
struct Demo {
    print_every: Option<Duration>,
    next_print: Duration,
    exit_after: Option<Duration>,
}

impl App for Demo {
    fn draw(&mut self, p: &mut Painter<'_>, frame: &FrameInfo) {
        let w = frame.size.width;
        let h = frame.size.height;
        let t = frame.time.as_secs_f64();

        // Backdrop: a warm diagonal gradient.
        p.fill_rect(
            Rect::from_xywh(0.0, 0.0, w, h),
            0.0,
            Brush::linear(
                Point::ZERO,
                Point::new(w, h),
                [
                    (0.0, Color::from_hex(0xFFF1E0)),
                    (1.0, Color::from_hex(0xFFDCEB)),
                ],
            ),
        );

        // Header.
        let margin = 56.0;
        let title = TextStyle::new(64.0)
            .weight(FontWeight::HEAVY)
            .color(INK)
            .letter_spacing(-1.5);
        let title_size = p.text("rugkit", Point::new(margin, 48.0), &title);
        let tagline = TextStyle::new(22.0)
            .weight(FontWeight::MEDIUM)
            .color(INK_SOFT);
        p.text(
            "The one framework that cares about visuals",
            Point::new(margin, 48.0 + title_size.height),
            &tagline,
        );

        // Cards.
        let gap = 28.0;
        let top = 200.0;
        let card_w = ((w - 2.0 * margin - 2.0 * gap) / 3.0).max(180.0);
        let card_h = 260.0;
        for (i, accent) in ACCENTS.iter().enumerate() {
            let x = margin + i as f64 * (card_w + gap);
            let card = Rect::from_xywh(x, top, card_w, card_h);
            draw_card(p, card, accent);
        }

        // Button with a tinted glow.
        let button = Rect::from_xywh(margin, top + card_h + 48.0, 220.0, 60.0);
        let violet = ACCENTS[1].from;
        p.shadow(
            button,
            30.0,
            &Shadow::new(violet.with_alpha(0.45), Vec2::new(0.0, 14.0), 28.0).with_spread(-4.0),
        );
        p.fill_rect(
            button,
            30.0,
            Brush::linear(
                button.origin(),
                Point::new(button.x1, button.y1),
                [(0.0, violet), (1.0, ACCENTS[1].to)],
            ),
        );
        p.stroke_rect(
            button.inset(0.75),
            29.25,
            1.5,
            Color::WHITE.with_alpha(0.25),
        );
        let label = TextStyle::new(19.0)
            .weight(FontWeight::BOLD)
            .color(Color::WHITE);
        let label_size = p.measure_text("Get started", None, &label);
        p.text(
            "Get started",
            Point::new(
                button.center().x - label_size.width / 2.0,
                button.center().y - label_size.height / 2.0,
            ),
            &label,
        );

        // Motion track: a ball springing between the ends, a stand-in until M1's real springs.
        let track = Rect::from_xywh(
            button.x1 + 40.0,
            button.y0 + 18.0,
            (w - margin - button.x1 - 40.0).max(120.0),
            24.0,
        );
        p.fill_rect(track, 12.0, INK.with_alpha(0.08));
        let ball_r = 18.0;
        // The spring overshoots by up to 10% at each end, so leave that much room inside the track.
        let travel = (track.width() - 2.0 * ball_r) / 1.2;
        let overshoot_room = travel * 0.1;
        let progress = spring_ping_pong(t, 1.6);
        let ball = Point::new(
            track.x0 + ball_r + overshoot_room + travel * progress,
            track.center().y,
        );
        p.shadow(
            Rect::from_xywh(ball.x - ball_r, ball.y - ball_r, 2.0 * ball_r, 2.0 * ball_r),
            ball_r,
            &Shadow::new(ACCENTS[0].from.with_alpha(0.4), Vec2::new(0.0, 8.0), 16.0),
        );
        p.fill_circle(
            ball,
            ball_r,
            Brush::linear(
                Point::new(ball.x - ball_r, ball.y - ball_r),
                Point::new(ball.x + ball_r, ball.y + ball_r),
                [(0.0, ACCENTS[0].from), (1.0, ACCENTS[0].to)],
            ),
        );

        // Weight ramp: shows the variable font axis is applied.
        let ramp_y = button.y1 + 40.0;
        let mut x = margin;
        for weight in [300.0, 400.0, 500.0, 600.0, 700.0, 800.0, 900.0] {
            let style = TextStyle::new(30.0).weight(FontWeight(weight)).color(INK);
            let size = p.text("Aa", Point::new(x, ramp_y), &style);
            x += size.width + 20.0;
        }
        let dense = TextStyle::new(15.0).family(FontFamily::Ui).color(INK_SOFT);
        p.text(
            "Dense text uses regular SF Pro: 12,480.50 · Groceries · 24 Nov 2026",
            Point::new(x + 12.0, ramp_y + 8.0),
            &dense,
        );

        draw_stats(p, frame);
        self.report(frame);
    }
}

impl Demo {
    fn report(&mut self, frame: &FrameInfo) {
        if let Some(every) = self.print_every
            && frame.time >= self.next_print
        {
            self.next_print = frame.time + every;
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
}

fn draw_card(p: &mut Painter<'_>, card: Rect, accent: &Accent) {
    let radius = 28.0;
    // Layered shadows: a tight contact shadow plus a wide, accent-tinted ambient one.
    p.shadow(
        card,
        radius,
        &Shadow::new(INK.with_alpha(0.08), Vec2::new(0.0, 2.0), 6.0),
    );
    p.shadow(
        card,
        radius,
        &Shadow::new(accent.from.with_alpha(0.22), Vec2::new(0.0, 22.0), 44.0).with_spread(-6.0),
    );
    p.fill_rect(card, radius, Color::WHITE);

    let chip = Rect::from_xywh(card.x0 + 24.0, card.y0 + 24.0, 56.0, 56.0);
    p.fill_rect(
        chip,
        18.0,
        Brush::linear(
            chip.origin(),
            Point::new(chip.x1, chip.y1),
            [(0.0, accent.from), (1.0, accent.to)],
        ),
    );
    p.fill_circle(chip.center(), 9.0, Color::WHITE.with_alpha(0.9));

    let title = TextStyle::new(24.0).weight(FontWeight::BOLD).color(INK);
    let title_size = p.text(
        accent.title,
        Point::new(card.x0 + 24.0, chip.y1 + 20.0),
        &title,
    );
    let body = TextStyle::new(16.0)
        .family(FontFamily::Ui)
        .color(INK_SOFT)
        .line_height(1.4);
    p.text_wrapped(
        accent.body,
        Point::new(card.x0 + 24.0, chip.y1 + 28.0 + title_size.height),
        card.width() - 48.0,
        &body,
    );
}

fn draw_stats(p: &mut Painter<'_>, frame: &FrameInfo) {
    let s = frame.stats;
    let text = format!(
        "{:.0} fps   cpu {:.2} ms   gpu {}   worst {:.2} ms",
        s.fps,
        s.cpu_ms,
        s.gpu_ms.map_or("–".into(), |g| format!("{g:.2} ms")),
        s.worst_ms
    );
    let style = TextStyle::new(13.0)
        .family(FontFamily::Ui)
        .weight(FontWeight::MEDIUM)
        .color(Color::WHITE);
    let size = p.measure_text(&text, None, &style);
    let pill = Rect::from_xywh(
        frame.size.width - size.width - 24.0 - 24.0,
        frame.size.height - size.height - 16.0 - 24.0,
        size.width + 24.0,
        size.height + 16.0,
    );
    p.fill_rect(pill, pill.height() / 2.0, INK.with_alpha(0.85));
    p.text(&text, Point::new(pill.x0 + 12.0, pill.y0 + 8.0), &style);
}

/// Goes 0 → 1 → 0 every `period` seconds, each leg settling like an underdamped spring.
fn spring_ping_pong(t: f64, period: f64) -> f64 {
    let leg = (t / period).floor() as i64;
    let local = t - leg as f64 * period;
    let (omega, zeta) = (14.0_f64, 0.32_f64);
    let wd = omega * (1.0 - zeta * zeta).sqrt();
    let settle = 1.0
        - (-zeta * omega * local).exp()
            * ((wd * local).cos() + zeta * omega / wd * (wd * local).sin());
    let settle = settle.clamp(-0.1, 1.1);
    if leg % 2 == 0 { settle } else { 1.0 - settle }
}

fn main() -> Result<(), rugkit::Error> {
    let env_secs = |key: &str| {
        std::env::var(key)
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .map(Duration::from_secs_f64)
    };
    let demo = Demo {
        print_every: std::env::var_os("M0_PRINT_STATS").map(|_| Duration::from_secs(2)),
        exit_after: env_secs("M0_EXIT_AFTER"),
        ..Demo::default()
    };
    rugkit::run(
        WindowOptions {
            title: "rugkit · M0".into(),
            size: Size::new(1180.0, 760.0),
            background: Color::from_hex(0xFFF1E0),
            measure_gpu: true,
            ..WindowOptions::default()
        },
        demo,
    )
}
