//! Renders the official 31-second, 1920×1080 @ 60fps **rugkit** launch advertising motion
//! video directly on the GPU using rugkit's own `Painter`, `Spring`, `Ui`, `IconName` and
//! `OffscreenRenderer`, piping frames straight into `ffmpeg` with a synthesized ambient
//! soundtrack.
//!
//! ```bash
//! cargo run --release --example render_promo -- docs/demo/rugkit_launch.mp4
//! ```

use std::f64::consts::TAU;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

use rugkit::prelude::*;
use rugkit::{
    Animated, FrameInfo, FrameStats, InputEvent, Motion, OffscreenRenderer, PointerButton,
    Transform, Ui, set_theme,
};

const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
const LOGICAL_W: f64 = 1280.0;
const LOGICAL_H: f64 = 720.0;
const SCALE: f64 = 1.5; // 1280 * 1.5 = 1920, 720 * 1.5 = 1080
const FPS: u32 = 60;
const TOTAL_SECS: f64 = 31.0;
const TOTAL_FRAMES: u32 = (TOTAL_SECS as u32) * FPS;

fn ease_out_back(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

fn ease_out_cubic(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn draw_centered_text(p: &mut Painter<'_>, s: &str, cx: f64, y: f64, style: &TextStyle) {
    let m = p.measure_text(s, None, style);
    p.text(s, Point::new(cx - m.width * 0.5, y), style);
}

fn draw_pill(
    p: &mut Painter<'_>,
    label: &str,
    cx: f64,
    y: f64,
    bg: Color,
    fg: Color,
    border: Color,
) {
    let style = TextStyle::new(15.0).weight(FontWeight::BOLD).color(fg);
    let m = p.measure_text(label, None, &style);
    let w = m.width + 32.0;
    let h = 36.0;
    let rect = Rect::from_xywh(cx - w * 0.5, y, w, h);
    p.fill_rect(rect, 999.0, bg);
    if border.a > 0.01 {
        p.stroke_rect(rect, 999.0, 1.2, border);
    }
    p.text(label, Point::new(cx - m.width * 0.5, y + 8.5), &style);
}

fn draw_caption_bar(p: &mut Painter<'_>, title: &str, pills: &[&str], alpha: f32) {
    draw_caption_bar_themed(p, title, pills, alpha, 0.0);
}

fn draw_caption_bar_themed(
    p: &mut Painter<'_>,
    title: &str,
    pills: &[&str],
    alpha: f32,
    light_mix: f32,
) {
    if alpha <= 0.01 {
        return;
    }
    let y_title = 618.0;
    let title_col = Color::from_hex(0xF5F3FF)
        .mix(Color::from_hex(0x1E1B3A), light_mix)
        .with_alpha(alpha);
    let st = TextStyle::new(22.0)
        .weight(FontWeight::BOLD)
        .color(title_col);
    draw_centered_text(p, title, LOGICAL_W * 0.5, y_title, &st);

    let total_w: f64 = pills
        .iter()
        .map(|s| {
            p.measure_text(s, None, &TextStyle::new(14.0).weight(FontWeight::BOLD))
                .width
                + 30.0
        })
        .sum::<f64>()
        + (pills.len().saturating_sub(1) as f64) * 10.0;
    let mut x = (LOGICAL_W - total_w) * 0.5;
    let y = 660.0;
    let pill_bg = Color::rgba(0.18, 0.16, 0.32, 0.85)
        .mix(Color::rgba(0.14, 0.12, 0.26, 0.92), light_mix)
        .with_alpha(0.90 * alpha);
    for &label in pills {
        let style = TextStyle::new(14.0)
            .weight(FontWeight::SEMIBOLD)
            .color(Color::from_hex(0xE8E4FF).with_alpha(alpha));
        let w = p.measure_text(label, None, &style).width + 28.0;
        let r = Rect::from_xywh(x, y, w, 32.0);
        p.fill_rect(r, 999.0, pill_bg);
        p.stroke_rect(r, 999.0, 1.0, Color::rgba(0.60, 0.52, 1.0, 0.35 * alpha));
        p.text(label, Point::new(x + 14.0, y + 7.0), &style);
        x += w + 10.0;
    }
}

fn draw_backdrop(p: &mut Painter<'_>, t: f64, light_mix: f32) {
    let dark_top = Color::from_hex(0x0D0B1A);
    let dark_bot = Color::from_hex(0x16122B);
    let light_top = Color::from_hex(0xFFF3E6);
    let light_bot = Color::from_hex(0xF9E6D4);
    let top = dark_top.mix(light_top, light_mix);
    let bot = dark_bot.mix(light_bot, light_mix);
    p.fill_rect(
        Rect::from_xywh(0.0, 0.0, LOGICAL_W, LOGICAL_H),
        0.0,
        Brush::linear(
            Point::ZERO,
            Point::new(0.0, LOGICAL_H),
            [(0.0, top), (1.0, bot)],
        ),
    );

    // Soft ambient drifting glows.
    let alpha = (1.0 - light_mix * 0.65) * 0.22;
    let o1 = Point::new(
        260.0 + (t * 0.5).sin() * 60.0,
        180.0 + (t * 0.4).cos() * 40.0,
    );
    let o2 = Point::new(
        1020.0 + (t * 0.45).cos() * 70.0,
        520.0 + (t * 0.35).sin() * 45.0,
    );
    let o3 = Point::new(
        640.0 + (t * 0.3).sin() * 80.0,
        320.0 + (t * 0.6).cos() * 35.0,
    );
    p.shadow(
        Rect::from_xywh(o1.x - 140.0, o1.y - 140.0, 280.0, 280.0),
        140.0,
        &Shadow::new(
            Color::from_hex(0x7B61FF).with_alpha(alpha),
            Vec2::ZERO,
            120.0,
        ),
    );
    p.shadow(
        Rect::from_xywh(o2.x - 150.0, o2.y - 150.0, 300.0, 300.0),
        150.0,
        &Shadow::new(
            Color::from_hex(0x00B894).with_alpha(alpha * 0.85),
            Vec2::ZERO,
            120.0,
        ),
    );
    p.shadow(
        Rect::from_xywh(o3.x - 130.0, o3.y - 130.0, 260.0, 260.0),
        130.0,
        &Shadow::new(
            Color::from_hex(0x2F80ED).with_alpha(alpha * 0.7),
            Vec2::ZERO,
            110.0,
        ),
    );
}

fn draw_window_chrome(p: &mut Painter<'_>, frame: Rect, title: &str, dark: bool) {
    let bg = if dark {
        Color::from_hex(0x141224)
    } else {
        Color::from_hex(0xFFF3E6)
    };
    let border = if dark {
        Color::rgba(0.95, 0.94, 1.0, 0.16)
    } else {
        Color::rgba(0.12, 0.10, 0.23, 0.14)
    };
    p.shadow(
        frame,
        24.0,
        &Shadow::new(Color::rgba(0.0, 0.0, 0.0, 0.45), Vec2::new(0.0, 18.0), 36.0),
    );
    p.fill_rect(frame, 24.0, bg);
    p.stroke_rect(frame, 24.0, 1.5, border);

    // macOS traffic lights.
    let cy = frame.y0 + 20.0;
    let colors = [0xFF5F57u32, 0xFEBC2E, 0x28C840];
    for (i, hex) in colors.into_iter().enumerate() {
        p.fill_circle(
            Point::new(frame.x0 + 22.0 + i as f64 * 18.0, cy),
            5.5,
            Color::from_hex(hex),
        );
    }
    let st = TextStyle::new(13.0)
        .weight(FontWeight::SEMIBOLD)
        .color(if dark {
            Color::from_hex(0xA6A1C4)
        } else {
            Color::from_hex(0x6B6687)
        });
    draw_centered_text(p, title, frame.center().x, frame.y0 + 12.0, &st);
}

fn draw_icon_at(p: &mut Painter<'_>, ic: IconName, center: Point, size: f64, color: Color) {
    let path = ic.path();
    let s = size / 24.0;
    let xform = Transform::scale_about(s, Point::ZERO).then(Transform::translate(Vec2::new(
        center.x - size * 0.5,
        center.y - size * 0.5,
    )));
    p.with_transform(xform, |p| {
        p.stroke_path(&path, 2.0, color);
    });
}

// ---- Scene 1: Kinetic Brand Intro (0.0s .. 4.0s) ----------------------------------------

fn draw_scene_intro(p: &mut Painter<'_>, local_t: f64) {
    let scale = 0.72 + 0.28 * ease_out_back(local_t / 0.85);
    let alpha = smoothstep(0.0, 0.45, local_t) as f32;
    let fade_out = (1.0 - smoothstep(3.4, 4.0, local_t)) as f32;
    let a = alpha * fade_out;

    // Decorative orbiting floating cards framing the title cleanly (never overlapping pills).
    let card_angles = [-0.55, 0.25, 2.85, 3.65];
    let card_colors = [0x7B61FFu32, 0x00B894, 0xFF5A5F, 0x2F80ED];
    let card_icons = [
        IconName::Layers,
        IconName::TrendingUp,
        IconName::Sliders,
        IconName::Heart,
    ];
    for i in 0..4 {
        let enter = ease_out_back((local_t - 0.15 * i as f64) / 0.75);
        let ang = card_angles[i] + local_t * 0.08;
        let rx = 490.0 * enter;
        let ry = 155.0 * enter;
        let cx = LOGICAL_W * 0.5 + ang.cos() * rx;
        let cy = 248.0 + ang.sin() * ry;
        let r = Rect::from_xywh(cx - 42.0, cy - 42.0, 84.0, 84.0);
        let col = Color::from_hex(card_colors[i]);
        p.fill_rect(r, 24.0, Color::rgba(0.14, 0.12, 0.24, 0.9 * a));
        p.stroke_rect(r, 24.0, 1.5, col.with_alpha(0.6 * a));
        draw_icon_at(p, card_icons[i], r.center(), 34.0, col.with_alpha(a));
    }

    p.with_transform(
        Transform::scale_about(scale, Point::new(LOGICAL_W * 0.5, 300.0)),
        |p| {
            // Glow behind logo
            p.shadow(
                Rect::from_xywh(LOGICAL_W * 0.5 - 190.0, 225.0, 380.0, 90.0),
                45.0,
                &Shadow::new(
                    Color::from_hex(0x7B61FF).with_alpha(0.35 * a),
                    Vec2::ZERO,
                    48.0,
                ),
            );
            let title_style = TextStyle::new(112.0)
                .weight(FontWeight::BLACK)
                .letter_spacing(-4.0)
                .color(Color::from_hex(0xFFFFFF).with_alpha(a));
            draw_centered_text(p, "rugkit", LOGICAL_W * 0.5, 200.0, &title_style);
        },
    );

    let sub_t = ease_out_cubic((local_t - 0.55) / 0.65);
    let sub_y = 356.0 + (1.0 - sub_t) * 26.0;
    let sub_a = (sub_t as f32) * fade_out;
    let sub_style = TextStyle::new(30.0)
        .weight(FontWeight::SEMIBOLD)
        .color(Color::from_hex(0xC9C3E8).with_alpha(sub_a));
    draw_centered_text(
        p,
        "The one framework that cares about visuals.",
        LOGICAL_W * 0.5,
        sub_y,
        &sub_style,
    );

    let badges = [
        ("Pure Rust", 0xFF5A5Fu32),
        ("vello · Metal GPU", 0x7B61FF),
        ("120fps Springs", 0x00B894),
        ("Zero Proc Macros", 0x2F80ED),
    ];
    let start_x = LOGICAL_W * 0.5 - 330.0;
    for (i, (lbl, hex)) in badges.into_iter().enumerate() {
        let bt = ease_out_back((local_t - 1.0 - i as f64 * 0.12) / 0.55);
        if bt > 0.02 {
            let cx = start_x + i as f64 * 220.0;
            let y = 444.0 + (1.0 - bt) * 28.0;
            let c = Color::from_hex(hex);
            draw_pill(
                p,
                lbl,
                cx,
                y,
                c.with_alpha(0.20 * fade_out),
                Color::WHITE.with_alpha(fade_out),
                c.with_alpha(0.75 * fade_out),
            );
        }
    }
}

// ---- Scene 2: Clean Builder Code + Live Interactive Counter Ui (4.0s .. 8.5s) -----------

fn draw_scene_builder(
    p: &mut Painter<'_>,
    local_t: f64,
    ui: &mut Ui,
    count: Signal<i32>,
    last_step: &mut usize,
) {
    let enter = ease_out_back(local_t / 0.75);
    let fade = (smoothstep(0.0, 0.35, local_t) * (1.0 - smoothstep(4.0, 4.5, local_t))) as f32;

    // Drive live counter clicks mid-spring to prove interruptible motion!
    let step_idx = (local_t / 0.55).floor() as usize;
    if step_idx != *last_step {
        *last_step = step_idx;
        const SCRIPT: [i32; 8] = [1, 2, 4, 6, 8, 3, -2, 5];
        count.set(SCRIPT[step_idx % SCRIPT.len()]);
    }

    // Left: Rust code card
    let code_rect = Rect::from_xywh(70.0 - (1.0 - enter) * 60.0, 96.0, 540.0, 470.0);
    p.fill_rect(code_rect, 26.0, Color::rgba(0.10, 0.09, 0.19, 0.96 * fade));
    p.stroke_rect(
        code_rect,
        26.0,
        1.5,
        Color::from_hex(0x7B61FF).with_alpha(0.45 * fade),
    );
    let hdr = TextStyle::new(15.0)
        .weight(FontWeight::BOLD)
        .color(Color::from_hex(0x9B85FF).with_alpha(fade));
    p.text(
        "src/main.rs  ·  Plain Rust Builder Chains",
        Point::new(code_rect.x0 + 24.0, code_rect.y0 + 22.0),
        &hdr,
    );

    let lines = [
        ("let count = signal(0);", 0x00E5A0u32),
        ("", 0xFFFFFF),
        ("column((", 0xF5F3FF),
        ("    text(move || count.get().to_string())", 0x9B85FF),
        ("        .size(96.0)", 0xC9C3E8),
        ("        .weight(FontWeight::BLACK)", 0xC9C3E8),
        ("        .color(tone),", 0xC9C3E8),
        ("    row((", 0xF5F3FF),
        ("        button(\"−\").secondary()", 0xFF9F1C),
        (
            "            .on_click(move || count.update(|c| *c -= 1)),",
            0xA6A1C4,
        ),
        ("        button(\"+\").primary()", 0x00E5A0),
        (
            "            .on_click(move || count.update(|c| *c += 1)),",
            0xA6A1C4,
        ),
        ("    )).gap(14.0),", 0xF5F3FF),
        ("))", 0xF5F3FF),
        (".padding(32.0)", 0xC9C3E8),
        (".radius(36.0)", 0xC9C3E8),
        (".elevation(10.0)", 0xC9C3E8),
    ];
    for (i, (line, hex)) in lines.into_iter().enumerate() {
        let st = TextStyle::new(14.5)
            .family(FontFamily::Monospace)
            .color(Color::from_hex(hex).with_alpha(fade));
        p.text(
            line,
            Point::new(code_rect.x0 + 24.0, code_rect.y0 + 62.0 + i as f64 * 23.0),
            &st,
        );
    }

    // Right: Live rendered Counter Ui inside macOS window chrome!
    let win_rect = Rect::from_xywh(650.0 + (1.0 - enter) * 60.0, 96.0, 560.0, 470.0);
    draw_window_chrome(p, win_rect, "rugkit · counter", true);

    let info = FrameInfo {
        size: Size::new(win_rect.width(), win_rect.height() - 36.0),
        scale: SCALE,
        time: Duration::from_secs_f64(local_t),
        dt: Duration::from_secs_f64(1.0 / f64::from(FPS)),
        stats: FrameStats::default(),
        reduce_motion: false,
        appearance: Appearance::new(ColorScheme::Dark, Some(Color::from_hex(0x7B61FF))),
    };
    p.with_clip(
        Rect::from_xywh(
            win_rect.x0,
            win_rect.y0 + 36.0,
            win_rect.width(),
            win_rect.height() - 36.0,
        ),
        20.0,
        |p| {
            p.with_transform(
                Transform::translate(Vec2::new(win_rect.x0, win_rect.y0 + 36.0)),
                |p| {
                    ui.draw(p, &info);
                },
            );
        },
    );

    draw_caption_bar(
        p,
        "Plain Rust builders. Fine-grained signals. Implicit interruptible springs.",
        &[
            "Copy Signal<T>",
            "Zero Tree Diffing",
            "Velocity-Preserving Springs",
        ],
        fade,
    );
}

// ---- Scene 3: Flagship #1 — Minty Personal Finance (8.5s .. 14.5s) ----------------------

fn draw_scene_finance(p: &mut Painter<'_>, local_t: f64, bar_anims: &mut [Animated<f64>; 5]) {
    let enter = ease_out_back(local_t / 0.7);
    let fade = (smoothstep(0.0, 0.35, local_t) * (1.0 - smoothstep(5.5, 6.0, local_t))) as f32;

    let win = Rect::from_xywh(90.0, 46.0 + (1.0 - enter) * 40.0, 1100.0, 550.0);
    draw_window_chrome(p, win, "Minty · Flagship Personal Finance", true);

    let inner_y = win.y0 + 40.0;
    let surface = Color::from_hex(0x1F1C36);
    let border = Color::rgba(0.95, 0.94, 1.0, 0.14);
    let accent = Color::from_hex(0x7B61FF);
    let positive = Color::from_hex(0x26D9B2);
    let negative = Color::from_hex(0xFF6B70);

    // Sidebar
    let sb = Rect::from_xywh(win.x0 + 12.0, inner_y, 185.0, win.height() - 52.0);
    p.fill_rect(sb, 18.0, surface);
    p.stroke_rect(sb, 18.0, 1.0, border);
    draw_icon_at(
        p,
        IconName::Wallet,
        Point::new(sb.x0 + 28.0, sb.y0 + 28.0),
        20.0,
        accent,
    );
    p.text(
        "Minty",
        Point::new(sb.x0 + 46.0, sb.y0 + 17.0),
        &TextStyle::new(19.0)
            .weight(FontWeight::BLACK)
            .color(Color::WHITE),
    );
    let navs = [
        ("Overview", IconName::Home),
        ("Transactions", IconName::CreditCard),
        ("Budgets", IconName::PieChart),
        ("Settings", IconName::Settings),
    ];
    let active_nav = if local_t < 3.8 { 0 } else { 1 };
    for (i, (lbl, ic)) in navs.into_iter().enumerate() {
        let nr = Rect::from_xywh(
            sb.x0 + 12.0,
            sb.y0 + 64.0 + i as f64 * 42.0,
            sb.width() - 24.0,
            34.0,
        );
        if i == active_nav {
            p.fill_rect(nr, 12.0, accent);
        }
        draw_icon_at(
            p,
            ic,
            Point::new(nr.x0 + 18.0, nr.center().y),
            16.0,
            if i == active_nav {
                Color::WHITE
            } else {
                Color::from_hex(0xA6A1C4)
            },
        );
        p.text(
            lbl,
            Point::new(nr.x0 + 36.0, nr.y0 + 8.0),
            &TextStyle::new(14.0)
                .weight(FontWeight::BOLD)
                .color(Color::WHITE),
        );
    }

    // Main content
    let mx = sb.x1 + 16.0;
    let mw = win.x1 - mx - 16.0;

    // 3 KPI cards
    let kw = (mw - 24.0) / 3.0;
    let kpis = [
        ("Net Balance", "$118,314.24", accent, IconName::Wallet),
        (
            "Total Income",
            "$147,131.96",
            positive,
            IconName::TrendingUp,
        ),
        (
            "Total Spending",
            "$28,817.72",
            negative,
            IconName::CreditCard,
        ),
    ];
    for (i, (title, val, col, ic)) in kpis.into_iter().enumerate() {
        let kr = Rect::from_xywh(mx + i as f64 * (kw + 12.0), inner_y, kw, 94.0);
        p.fill_rect(kr, 18.0, surface);
        p.stroke_rect(kr, 18.0, 1.0, border);
        p.text(
            title,
            Point::new(kr.x0 + 16.0, kr.y0 + 14.0),
            &TextStyle::new(12.5).color(Color::from_hex(0xA6A1C4)),
        );
        draw_icon_at(p, ic, Point::new(kr.x1 - 24.0, kr.y0 + 22.0), 17.0, col);
        p.text(
            val,
            Point::new(kr.x0 + 16.0, kr.y0 + 44.0),
            &TextStyle::new(24.0)
                .weight(FontWeight::BLACK)
                .color(Color::WHITE),
        );
    }

    // Animated spending bars + cashflow line chart
    let chart_y = inner_y + 108.0;
    let left_c = Rect::from_xywh(mx, chart_y, mw * 0.52, 198.0);
    let right_c = Rect::from_xywh(left_c.x1 + 14.0, chart_y, mw - left_c.width() - 14.0, 198.0);
    p.fill_rect(left_c, 20.0, surface);
    p.stroke_rect(left_c, 20.0, 1.0, border);
    p.text(
        "Spending by Category",
        Point::new(left_c.x0 + 16.0, left_c.y0 + 14.0),
        &TextStyle::new(14.0)
            .weight(FontWeight::BOLD)
            .color(Color::WHITE),
    );

    let targets = if local_t < 2.2 {
        [58.0, 32.0, 74.0, 92.0, 48.0]
    } else {
        [82.0, 56.0, 48.0, 68.0, 88.0]
    };
    let bar_colors = [0x7B61FFu32, 0xFF9F1C, 0x2F80ED, 0xFF5A5F, 0xE84393];
    let cats = ["Groceries", "Dining", "Tech", "Travel", "Health"];
    for i in 0..5 {
        bar_anims[i].set_target(targets[i]);
        bar_anims[i].step(1.0 / f64::from(FPS), Motion::Spring(Spring::BOUNCY), false);
        let bh = (bar_anims[i].value() / 100.0) * 116.0;
        let bx = left_c.x0 + 28.0 + i as f64 * 82.0;
        let by = left_c.y1 - 30.0 - bh;
        let c = Color::from_hex(bar_colors[i]);
        p.fill_rect(
            Rect::from_xywh(bx, by, 44.0, bh),
            10.0,
            Brush::linear(
                Point::new(bx, by),
                Point::new(bx, by + bh),
                [(0.0, c), (1.0, c.with_alpha(0.5))],
            ),
        );
        draw_centered_text(
            p,
            cats[i],
            bx + 22.0,
            left_c.y1 - 24.0,
            &TextStyle::new(11.5).color(Color::from_hex(0xA6A1C4)),
        );
    }

    p.fill_rect(right_c, 20.0, surface);
    p.stroke_rect(right_c, 20.0, 1.0, border);
    p.text(
        "6-Month Cashflow",
        Point::new(right_c.x0 + 16.0, right_c.y0 + 14.0),
        &TextStyle::new(14.0)
            .weight(FontWeight::BOLD)
            .color(Color::WHITE),
    );
    let draw_prog = ease_out_cubic(local_t / 1.5);
    let curve_vals = [0.45, 0.62, 0.58, 0.78, 0.84, 0.93];
    let mut area = Path::new();
    let mut line = Path::new();
    let base_y = right_c.y1 - 28.0;
    let mut pts = Vec::new();
    for (i, &v) in curve_vals.iter().enumerate() {
        let x = right_c.x0 + 26.0 + (right_c.width() - 52.0) * (i as f64 / 5.0);
        let y = base_y - v * 115.0 * draw_prog;
        pts.push(Point::new(x, y));
    }
    area.move_to(Point::new(pts[0].x, base_y));
    line.move_to(pts[0]);
    for &pt in &pts {
        area.line_to(pt);
        line.line_to(pt);
    }
    area.line_to(Point::new(pts[5].x, base_y));
    area.close();
    p.fill_path(
        &area,
        Brush::linear(
            Point::new(right_c.x0, right_c.y0 + 40.0),
            Point::new(right_c.x0, base_y),
            [
                (0.0, positive.with_alpha(0.32)),
                (1.0, positive.with_alpha(0.02)),
            ],
        ),
    );
    p.stroke_path(&line, 3.0, positive);
    for &pt in &pts {
        p.fill_circle(pt, 5.5, surface);
        p.fill_circle(pt, 4.0, positive);
    }

    // Virtualised transactions table scrolling smoothly
    let tbl_r = Rect::from_xywh(mx, chart_y + 210.0, mw, 180.0);
    p.fill_rect(tbl_r, 16.0, surface);
    p.stroke_rect(tbl_r, 16.0, 1.0, border);
    p.fill_rect(
        Rect::from_xywh(tbl_r.x0, tbl_r.y0, tbl_r.width(), 32.0),
        16.0,
        Color::from_hex(0x2A2647),
    );
    let hdr_st = TextStyle::new(12.0)
        .weight(FontWeight::BOLD)
        .color(Color::from_hex(0xA6A1C4));
    p.text("#", Point::new(tbl_r.x0 + 16.0, tbl_r.y0 + 8.0), &hdr_st);
    p.text(
        "Merchant",
        Point::new(tbl_r.x0 + 86.0, tbl_r.y0 + 8.0),
        &hdr_st,
    );
    p.text(
        "Category",
        Point::new(tbl_r.x0 + 360.0, tbl_r.y0 + 8.0),
        &hdr_st,
    );
    p.text(
        "Amount",
        Point::new(tbl_r.x1 - 105.0, tbl_r.y0 + 8.0),
        &hdr_st,
    );

    let scroll_px = (local_t * 62.0) % 240.0;
    let body_r = Rect::from_xywh(
        tbl_r.x0,
        tbl_r.y0 + 32.0,
        tbl_r.width(),
        tbl_r.height() - 32.0,
    );
    let sample_rows = [
        (
            "0001",
            "Acme Corp Payroll",
            "Income",
            "+$4,250.00",
            0x00B894u32,
        ),
        (
            "0002",
            "Whole Foods Market",
            "Groceries",
            "-$148.50",
            0x7B61FF,
        ),
        ("0003", "Apple Store", "Tech", "-$249.00", 0x2F80ED),
        ("0004", "Delta Air Lines", "Travel", "-$384.00", 0xFF5A5F),
        ("0005", "Blue Bottle Coffee", "Dining", "-$8.75", 0xFF9F1C),
        ("0006", "Vanguard Dividend", "Income", "+$642.00", 0x00B894),
        ("0007", "Equinox Fitness", "Health", "-$210.00", 0xE84393),
        ("0008", "Trader Joe's", "Groceries", "-$94.20", 0x7B61FF),
    ];
    p.with_clip(body_r, 8.0, |p| {
        for i in 0..12 {
            let (id, merch, cat, amt, chex) = sample_rows[i % sample_rows.len()];
            let ry = body_r.y0 + i as f64 * 34.0 - scroll_px;
            if ry + 34.0 < body_r.y0 || ry > body_r.y1 {
                continue;
            }
            if i % 2 == 1 {
                p.fill_rect(
                    Rect::from_xywh(body_r.x0, ry, body_r.width(), 34.0),
                    0.0,
                    Color::rgba(1.0, 1.0, 1.0, 0.025),
                );
            }
            let st = TextStyle::new(13.0)
                .family(FontFamily::Ui)
                .color(Color::WHITE);
            p.text(id, Point::new(body_r.x0 + 16.0, ry + 8.0), &st);
            p.text(merch, Point::new(body_r.x0 + 86.0, ry + 8.0), &st);
            let cc = Color::from_hex(chex);
            p.fill_rect(
                Rect::from_xywh(body_r.x0 + 356.0, ry + 6.0, 86.0, 22.0),
                999.0,
                cc.with_alpha(0.18),
            );
            draw_centered_text(
                p,
                cat,
                body_r.x0 + 399.0,
                ry + 9.0,
                &TextStyle::new(11.5).weight(FontWeight::BOLD).color(cc),
            );
            let is_pos = amt.starts_with('+');
            p.text(
                amt,
                Point::new(body_r.x1 - 110.0, ry + 8.0),
                &TextStyle::new(13.0)
                    .weight(FontWeight::BOLD)
                    .color(if is_pos { positive } else { Color::WHITE }),
            );
        }
    });

    draw_caption_bar(
        p,
        "Flagship #1 · Minty Personal Finance",
        &[
            "100k-Row Virtualised Table",
            "Animated Canvas Charts",
            "Budgets & Modals",
            "Local JSON",
        ],
        fade,
    );
}

// ---- Scene 4: Flagship #2 — Pulse Studio (14.5s .. 20.5s) -------------------------------

fn draw_scene_studio(p: &mut Painter<'_>, local_t: f64) {
    let enter = ease_out_back(local_t / 0.7);
    let fade = (smoothstep(0.0, 0.35, local_t) * (1.0 - smoothstep(5.5, 6.0, local_t))) as f32;

    let win = Rect::from_xywh(90.0, 46.0 + (1.0 - enter) * 40.0, 1100.0, 550.0);
    draw_window_chrome(p, win, "Pulse Studio · Flagship Creative Workstation", true);

    let surface = Color::from_hex(0x1F1C36);
    let border = Color::rgba(0.95, 0.94, 1.0, 0.14);
    let accent = Color::from_hex(0x2F80ED).mix(
        Color::from_hex(0x7B61FF),
        ((local_t * 0.6).sin() * 0.5 + 0.5) as f32,
    );

    // Real-time 24-band FFT Spectrum + Bézier Oscilloscope
    let spec_card = Rect::from_xywh(win.x0 + 20.0, win.y0 + 44.0, 560.0, 260.0);
    p.fill_rect(spec_card, 22.0, surface);
    p.stroke_rect(spec_card, 22.0, 1.0, border);
    p.text(
        "Live Spectrum & Oscilloscope · 124 BPM",
        Point::new(spec_card.x0 + 18.0, spec_card.y0 + 14.0),
        &TextStyle::new(14.0)
            .weight(FontWeight::BOLD)
            .color(Color::WHITE),
    );
    let screen = Rect::from_xywh(
        spec_card.x0 + 16.0,
        spec_card.y0 + 42.0,
        spec_card.width() - 32.0,
        spec_card.height() - 56.0,
    );
    p.fill_rect(screen, 14.0, Color::from_hex(0x0D0B1A));

    let cur_step = ((local_t * 4.13) as usize) % 8;
    let ph = local_t * 5.2;
    let bands = 24usize;
    let sw = (screen.width() - 24.0) / bands as f64;
    for b in 0..bands {
        let bf = b as f64 / bands as f64;
        let beat = if cur_step.is_multiple_of(2) && b < 7 {
            1.0
        } else {
            0.68
        };
        let amp = ((0.20 + 0.68 * (ph * (1.2 + bf * 2.8) + b as f64 * 0.5).sin().abs()) * beat)
            .clamp(0.1, 0.95);
        let bh = (screen.height() - 26.0) * amp;
        let bx = screen.x0 + 12.0 + b as f64 * sw + sw * 0.16;
        let by = screen.y1 - 12.0 - bh;
        let top = accent.mix(Color::from_hex(0x00E5A0), bf as f32);
        p.fill_rect(
            Rect::from_xywh(bx, by, sw * 0.68, bh),
            5.0,
            Brush::linear(
                Point::new(bx, by),
                Point::new(bx, by + bh),
                [(0.0, top), (1.0, accent.with_alpha(0.3))],
            ),
        );
        p.fill_circle(Point::new(bx + sw * 0.34, by - 5.0), 2.3, top);
    }

    let mut osc = Path::new();
    for s in 0..64 {
        let u = s as f64 / 63.0;
        let x = screen.x0 + 12.0 + u * (screen.width() - 24.0);
        let y = screen.center().y
            + (u * TAU * 3.0 + ph * 1.8).sin() * 44.0 * (u * std::f64::consts::PI).sin();
        if s == 0 {
            osc.move_to(Point::new(x, y));
        } else {
            osc.line_to(Point::new(x, y));
        }
    }
    p.stroke_path(&osc, 6.0, accent.with_alpha(0.3));
    p.stroke_path(&osc, 2.5, Color::WHITE);

    // 4×8 Step Sequencer
    let seq_card = Rect::from_xywh(spec_card.x1 + 16.0, win.y0 + 44.0, 484.0, 260.0);
    p.fill_rect(seq_card, 22.0, surface);
    p.stroke_rect(seq_card, 22.0, 1.0, border);
    p.text(
        "Step Sequencer",
        Point::new(seq_card.x0 + 18.0, seq_card.y0 + 14.0),
        &TextStyle::new(14.0)
            .weight(FontWeight::BOLD)
            .color(Color::WHITE),
    );
    let pat = [
        [true, false, false, false, true, false, true, false],
        [false, false, true, false, false, false, true, false],
        [true, true, true, true, true, true, true, true],
        [true, false, false, true, false, true, false, true],
    ];
    let tcols = [0xFF5A5Fu32, 0xFF9F1C, 0x00B894, 0x7B61FF];
    let tnames = ["Kick", "Snare", "Hi-Hat", "Synth"];
    for tr in 0..4 {
        let ry = seq_card.y0 + 52.0 + tr as f64 * 48.0;
        let tc = Color::from_hex(tcols[tr]);
        p.text(
            tnames[tr],
            Point::new(seq_card.x0 + 18.0, ry + 8.0),
            &TextStyle::new(13.0).weight(FontWeight::BOLD).color(tc),
        );
        for (st, &on) in pat[tr].iter().enumerate() {
            let px = seq_card.x0 + 78.0 + st as f64 * 48.0;
            let active = st == cur_step;
            let pad = Rect::from_xywh(px, ry, 40.0, 36.0);
            let pad = if on && active { pad.inset(-2.5) } else { pad };
            let bg = if on {
                if active {
                    tc.mix(Color::WHITE, 0.3)
                } else {
                    tc
                }
            } else if active {
                Color::rgba(1.0, 1.0, 1.0, 0.16)
            } else {
                Color::rgba(1.0, 1.0, 1.0, 0.06)
            };
            p.fill_rect(pad, 10.0, bg);
        }
    }

    // Bottom: Mixer Sliders + Stems Mini-Waveforms
    let bot_y = spec_card.y1 + 16.0;
    let mix_card = Rect::from_xywh(win.x0 + 20.0, bot_y, 500.0, 212.0);
    p.fill_rect(mix_card, 22.0, surface);
    p.stroke_rect(mix_card, 22.0, 1.0, border);
    p.text(
        "Synth & FX Rack",
        Point::new(mix_card.x0 + 18.0, mix_card.y0 + 14.0),
        &TextStyle::new(14.0)
            .weight(FontWeight::BOLD)
            .color(Color::WHITE),
    );
    let sliders = [
        ("Cutoff", 0.55 + 0.3 * (local_t * 1.1).sin()),
        ("Resonance", 0.48 + 0.22 * (local_t * 0.9 + 1.0).cos()),
        ("Reverb", 0.38 + 0.25 * (local_t * 0.7 + 2.0).sin()),
        ("Drive", 0.62 + 0.18 * (local_t * 1.3).cos()),
    ];
    for (i, (lbl, frac)) in sliders.into_iter().enumerate() {
        let sy = mix_card.y0 + 54.0 + i as f64 * 36.0;
        p.text(
            lbl,
            Point::new(mix_card.x0 + 18.0, sy),
            &TextStyle::new(13.0).color(Color::from_hex(0xA6A1C4)),
        );
        let tx = mix_card.x0 + 112.0;
        let tw = 310.0;
        p.fill_rect(
            Rect::from_xywh(tx, sy + 5.0, tw, 8.0),
            4.0,
            Color::rgba(1.0, 1.0, 1.0, 0.12),
        );
        p.fill_rect(Rect::from_xywh(tx, sy + 5.0, tw * frac, 8.0), 4.0, accent);
        p.fill_circle(Point::new(tx + tw * frac, sy + 9.0), 9.5, Color::WHITE);
        p.text(
            &format!("{:.0}%", frac * 100.0),
            Point::new(tx + tw + 14.0, sy),
            &TextStyle::new(13.0).weight(FontWeight::BOLD).color(accent),
        );
    }

    let stems_card = Rect::from_xywh(mix_card.x1 + 16.0, bot_y, 544.0, 212.0);
    p.fill_rect(stems_card, 22.0, surface);
    p.stroke_rect(stems_card, 22.0, 1.0, border);
    p.text(
        "Track Stems · Custom Table Cells",
        Point::new(stems_card.x0 + 18.0, stems_card.y0 + 14.0),
        &TextStyle::new(14.0)
            .weight(FontWeight::BOLD)
            .color(Color::WHITE),
    );
    let stem_names = [
        ("Neon Kick 808", 0xFF5A5Fu32, 11usize),
        ("Crisp Clap & Rim", 0xFF9F1C, 23),
        ("Velvet Pad Chords", 0x7B61FF, 37),
        ("Glass Arp Lead", 0x00B894, 53),
    ];
    for (r, (name, hex, seed)) in stem_names.into_iter().enumerate() {
        let ry = stems_card.y0 + 50.0 + r as f64 * 38.0;
        p.text(
            name,
            Point::new(stems_card.x0 + 18.0, ry + 6.0),
            &TextStyle::new(13.0).color(Color::WHITE),
        );
        let col = Color::from_hex(hex);
        for b in 0..24 {
            let h = 5.0 + (((b * 17 + seed * 7) % 11) as f64) * 1.9;
            p.fill_rect(
                Rect::from_xywh(
                    stems_card.x0 + 210.0 + b as f64 * 12.0,
                    ry + 14.0 - h * 0.5,
                    5.0,
                    h,
                ),
                2.5,
                col.with_alpha(0.88),
            );
        }
    }

    draw_caption_bar(
        p,
        "Flagship #2 · Pulse Studio Workstation",
        &[
            "60/120fps Reactive Canvas",
            "Step Sequencer",
            "Custom Waveforms",
            "Zero Lag",
        ],
        fade,
    );
}

// ---- Scene 5: Light <-> Dark Morph + Inspector + Core Widgets (20.5s .. 25.0s) ----------

fn draw_scene_theme_inspector(p: &mut Painter<'_>, local_t: f64) -> f32 {
    let fade = (smoothstep(0.0, 0.35, local_t) * (1.0 - smoothstep(4.0, 4.5, local_t))) as f32;
    let light_mix =
        smoothstep(0.8, 1.9, local_t) as f32 * (1.0 - smoothstep(3.3, 4.2, local_t) as f32);
    let show_inspector = local_t > 2.1;

    let bg = Color::from_hex(0x1F1C36).mix(Color::WHITE, light_mix);
    let ink = Color::from_hex(0xF5F3FF).mix(Color::from_hex(0x1E1B3A), light_mix);
    let soft = Color::from_hex(0xA6A1C4).mix(Color::from_hex(0x6B6687), light_mix);
    let accent = Color::from_hex(0x9B85FF).mix(Color::from_hex(0x7B61FF), light_mix);

    let win = Rect::from_xywh(130.0, 56.0, 1020.0, 530.0);
    draw_window_chrome(
        p,
        win,
        "rugkit · Widget Gallery & ⌘⌥I Inspector",
        light_mix < 0.5,
    );

    // 4 gallery cards morphing smoothly between Dark & Light
    let cards = [
        Rect::from_xywh(win.x0 + 24.0, win.y0 + 52.0, 474.0, 210.0),
        Rect::from_xywh(win.x0 + 520.0, win.y0 + 52.0, 474.0, 210.0),
        Rect::from_xywh(win.x0 + 24.0, win.y0 + 282.0, 474.0, 210.0),
        Rect::from_xywh(win.x0 + 520.0, win.y0 + 282.0, 474.0, 210.0),
    ];
    let titles = [
        "Buttons, Pills & Lucide Icons",
        "Checkboxes, Toggles, Radios & Sliders",
        "Subtree Coral Theme & RTL Mirroring",
        "CJK IME TextInput & AccessKit VoiceOver",
    ];
    for (i, (&cr, &title)) in cards.iter().zip(titles.iter()).enumerate() {
        p.fill_rect(cr, 24.0, bg);
        p.stroke_rect(cr, 24.0, 1.0, ink.with_alpha(0.14));
        p.text(
            title,
            Point::new(cr.x0 + 20.0, cr.y0 + 18.0),
            &TextStyle::new(15.0).weight(FontWeight::BOLD).color(ink),
        );

        match i {
            0 => {
                draw_pill(
                    p,
                    "Primary",
                    cr.x0 + 78.0,
                    cr.y0 + 62.0,
                    accent,
                    Color::WHITE,
                    Color::TRANSPARENT,
                );
                draw_pill(
                    p,
                    "Secondary",
                    cr.x0 + 205.0,
                    cr.y0 + 62.0,
                    bg,
                    ink,
                    ink.with_alpha(0.18),
                );
                draw_pill(
                    p,
                    "Coral",
                    cr.x0 + 325.0,
                    cr.y0 + 62.0,
                    Color::from_hex(0xFF5A5F),
                    Color::WHITE,
                    Color::TRANSPARENT,
                );
                let icons = [
                    IconName::Heart,
                    IconName::Star,
                    IconName::Wallet,
                    IconName::TrendingUp,
                    IconName::Settings,
                    IconName::Bell,
                ];
                for (k, ic) in icons.into_iter().enumerate() {
                    draw_icon_at(
                        p,
                        ic,
                        Point::new(cr.x0 + 40.0 + k as f64 * 52.0, cr.y0 + 144.0),
                        26.0,
                        accent,
                    );
                }
            }
            1 => {
                // Animated toggle + slider
                let tog = smoothstep(0.6, 1.4, local_t);
                p.fill_rect(
                    Rect::from_xywh(cr.x0 + 24.0, cr.y0 + 64.0, 56.0, 32.0),
                    999.0,
                    accent,
                );
                p.fill_circle(
                    Point::new(cr.x0 + 40.0 + tog * 24.0, cr.y0 + 80.0),
                    12.0,
                    Color::WHITE,
                );
                p.text(
                    "Animated Springs",
                    Point::new(cr.x0 + 96.0, cr.y0 + 71.0),
                    &TextStyle::new(15.0).color(ink),
                );

                let sl = 0.25 + 0.65 * smoothstep(0.5, 2.2, local_t);
                p.fill_rect(
                    Rect::from_xywh(cr.x0 + 24.0, cr.y0 + 138.0, 380.0, 8.0),
                    4.0,
                    ink.with_alpha(0.14),
                );
                p.fill_rect(
                    Rect::from_xywh(cr.x0 + 24.0, cr.y0 + 138.0, 380.0 * sl, 8.0),
                    4.0,
                    accent,
                );
                p.fill_circle(
                    Point::new(cr.x0 + 24.0 + 380.0 * sl, cr.y0 + 142.0),
                    11.0,
                    Color::WHITE,
                );
            }
            2 => {
                let coral = Color::from_hex(0xFF5A5F);
                draw_pill(
                    p,
                    "Coral Subtree",
                    cr.x0 + 96.0,
                    cr.y0 + 66.0,
                    coral,
                    Color::WHITE,
                    Color::TRANSPARENT,
                );
                draw_pill(
                    p,
                    "Second",
                    cr.x0 + 70.0,
                    cr.y0 + 132.0,
                    coral,
                    Color::WHITE,
                    Color::TRANSPARENT,
                );
                draw_pill(
                    p,
                    "First",
                    cr.x0 + 170.0,
                    cr.y0 + 132.0,
                    bg,
                    ink,
                    ink.with_alpha(0.18),
                );
                p.text(
                    "RTL mirrored →",
                    Point::new(cr.x0 + 236.0, cr.y0 + 141.0),
                    &TextStyle::new(14.0).color(soft),
                );
            }
            _ => {
                let inp = Rect::from_xywh(cr.x0 + 22.0, cr.y0 + 64.0, 420.0, 44.0);
                p.fill_rect(inp, 14.0, bg);
                p.stroke_rect(inp, 14.0, 2.0, accent);
                p.text(
                    "Hello 日本語 · CJK IME Ready!",
                    Point::new(inp.x0 + 16.0, inp.y0 + 12.0),
                    &TextStyle::new(15.0).family(FontFamily::Ui).color(ink),
                );
                p.text(
                    "Full VoiceOver & Headless Harness API",
                    Point::new(cr.x0 + 24.0, cr.y0 + 136.0),
                    &TextStyle::new(14.0).color(soft),
                );
            }
        }

        // Inspector bounds overlay when active!
        if show_inspector {
            p.stroke_rect(cr, 4.0, 1.5, Color::from_hex(0x00D1FF).with_alpha(0.85));
        }
    }

    if show_inspector {
        let hud = Rect::from_xywh(win.x1 - 248.0, win.y1 - 156.0, 276.0, 148.0);
        p.shadow(
            hud,
            18.0,
            &Shadow::new(Color::rgba(0.0, 0.0, 0.0, 0.40), Vec2::new(0.0, 10.0), 24.0),
        );
        p.fill_rect(hud, 18.0, Color::rgba(0.08, 0.07, 0.14, 0.96));
        p.stroke_rect(hud, 18.0, 1.5, accent);
        p.text(
            "rugkit inspector  (⌘⌥I)",
            Point::new(hud.x0 + 14.0, hud.y0 + 12.0),
            &TextStyle::new(14.0).weight(FontWeight::BOLD).color(accent),
        );
        let mono = TextStyle::new(12.0)
            .family(FontFamily::Monospace)
            .color(Color::WHITE);
        p.text(
            "FPS 120.0  CPU 1.90ms  GPU 7.76ms",
            Point::new(hud.x0 + 14.0, hud.y0 + 38.0),
            &mono,
        );
        p.text(
            "Nodes: 84   Bounds: ON",
            Point::new(hud.x0 + 14.0, hud.y0 + 62.0),
            &mono,
        );
        p.text(
            "Motion: 0.25x (Slow-Mo)",
            Point::new(hud.x0 + 14.0, hud.y0 + 86.0),
            &mono,
        );
        p.text(
            "[B] bounds [S] slow-mo [L] theme",
            Point::new(hud.x0 + 14.0, hud.y0 + 116.0),
            &TextStyle::new(11.5).color(Color::from_hex(0xA6A1C4)),
        );
    }

    draw_caption_bar_themed(
        p,
        "Animated Light & Dark · Subtree Themes · RTL · ⌘⌥I Inspector",
        &[
            "20+ Core Widgets",
            "VoiceOver AccessKit",
            "CJK IME & Clipboard",
            "Headless Harness",
        ],
        fade,
        light_mix,
    );
    light_mix
}

// ---- Scene 6: Hard Performance Numbers (25.0s .. 28.0s) ---------------------------------

fn draw_scene_perf(p: &mut Painter<'_>, local_t: f64) {
    let fade = (smoothstep(0.0, 0.3, local_t) * (1.0 - smoothstep(2.6, 3.0, local_t))) as f32;

    draw_centered_text(
        p,
        "Measured on Apple Silicon. Built for 120fps.",
        LOGICAL_W * 0.5,
        104.0,
        &TextStyle::new(38.0)
            .weight(FontWeight::BLACK)
            .color(Color::WHITE.with_alpha(fade)),
    );

    let stats = [
        (
            "< 8 ms",
            "Flagship Frame Time",
            "1.9ms CPU · 7.7ms GPU",
            0x7B61FFu32,
        ),
        (
            "139 ms",
            "Cold Start",
            "Launch to first presented frame",
            0x00B894,
        ),
        (
            "7.9 MB",
            "Release Binary",
            "Stripped hello-world executable",
            0xFF5A5F,
        ),
        (
            "100k+",
            "Virtualised Rows",
            "Plus cargo rugkit dev hot-reload",
            0x2F80ED,
        ),
    ];

    for (i, (big, label, sub, hex)) in stats.into_iter().enumerate() {
        let enter = ease_out_back((local_t - i as f64 * 0.14) / 0.55);
        let col = i % 2;
        let row = i / 2;
        let cx = 350.0 + col as f64 * 580.0;
        let cy = 285.0 + row as f64 * 205.0 + (1.0 - enter) * 35.0;
        let r = Rect::from_xywh(cx - 255.0, cy - 82.0, 510.0, 164.0);
        let accent = Color::from_hex(hex);

        p.fill_rect(r, 28.0, Color::rgba(0.12, 0.11, 0.21, 0.96 * fade));
        p.stroke_rect(r, 28.0, 1.5, accent.with_alpha(0.55 * fade));
        p.text(
            big,
            Point::new(r.x0 + 30.0, r.y0 + 22.0),
            &TextStyle::new(56.0)
                .weight(FontWeight::BLACK)
                .letter_spacing(-2.0)
                .color(accent.with_alpha(fade)),
        );
        p.text(
            label,
            Point::new(r.x0 + 30.0, r.y0 + 96.0),
            &TextStyle::new(20.0)
                .weight(FontWeight::BOLD)
                .color(Color::WHITE.with_alpha(fade)),
        );
        p.text(
            sub,
            Point::new(r.x0 + 30.0, r.y0 + 126.0),
            &TextStyle::new(14.0).color(Color::from_hex(0xA6A1C4).with_alpha(fade)),
        );
    }
}

// ---- Scene 7: Finale Outro (28.0s .. 31.0s) ---------------------------------------------

fn draw_scene_outro(p: &mut Painter<'_>, local_t: f64) {
    let enter = ease_out_back(local_t / 0.7);
    let alpha = smoothstep(0.0, 0.4, local_t) as f32;

    let icons = [
        IconName::Layers,
        IconName::Wallet,
        IconName::Sliders,
        IconName::TrendingUp,
        IconName::Star,
        IconName::Heart,
    ];
    for (i, ic) in icons.into_iter().enumerate() {
        let ang = (i as f64 / 6.0) * TAU + local_t * 0.35;
        let pt = Point::new(
            LOGICAL_W * 0.5 + ang.cos() * 340.0 * enter,
            LOGICAL_H * 0.46 + ang.sin() * 190.0 * enter,
        );
        draw_icon_at(
            p,
            ic,
            pt,
            30.0,
            Color::from_hex(0x9B85FF).with_alpha(0.55 * alpha),
        );
    }

    p.with_transform(
        Transform::scale_about(0.8 + 0.2 * enter, Point::new(LOGICAL_W * 0.5, 310.0)),
        |p| {
            draw_centered_text(
                p,
                "rugkit 0.1",
                LOGICAL_W * 0.5,
                215.0,
                &TextStyle::new(98.0)
                    .weight(FontWeight::BLACK)
                    .letter_spacing(-3.0)
                    .color(Color::WHITE.with_alpha(alpha)),
            );
        },
    );

    draw_centered_text(
        p,
        "Build Rust desktop apps that feel alive.",
        LOGICAL_W * 0.5,
        350.0,
        &TextStyle::new(28.0)
            .weight(FontWeight::SEMIBOLD)
            .color(Color::from_hex(0xC9C3E8).with_alpha(alpha)),
    );

    draw_pill(
        p,
        "github.com/Shadrous/rugkit  ·  MIT OR Apache-2.0",
        LOGICAL_W * 0.5,
        432.0,
        Color::from_hex(0x7B61FF).with_alpha(0.95 * alpha),
        Color::WHITE.with_alpha(alpha),
        Color::TRANSPARENT,
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "docs/demo/rugkit_launch.mp4".to_string());
    if let Some(parent) = std::path::Path::new(&out_path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    set_theme(Theme::PLAYFUL_DARK.with_accent(Color::from_hex(0x7B61FF)));

    // Build a real live Counter Ui for Scene 2.
    let count = signal(0i32);
    let mut counter_ui = Ui::new(move || {
        let tone = move || match count.get() {
            0 => theme().ink,
            c if c > 0 => theme().accent,
            _ => theme().negative,
        };
        column((
            text(move || count.get().to_string())
                .size(96.0)
                .weight(FontWeight::BLACK)
                .letter_spacing(-3.0)
                .color(tone),
            row((column(())
                .height(12.0)
                .radius(6.0)
                .width(move || {
                    (260.0 * f64::from(count.get().unsigned_abs().min(10)) / 10.0).max(12.0)
                })
                .background(tone),))
            .width(260.0)
            .height(12.0)
            .radius(6.0)
            .background(|| theme().ink.with_alpha(0.10))
            .justify(Justify::Start),
            row((
                button("−")
                    .secondary()
                    .on_click(move || count.update(|c| *c -= 1)),
                button("Reset").secondary().on_click(move || count.set(0)),
                button("+").on_click(move || count.update(|c| *c += 1)),
            ))
            .gap(12.0),
        ))
        .gap(20.0)
        .padding((30.0, 42.0))
        .background(|| theme().surface)
        .radius(34.0)
        .elevation(10.0)
        .shadow_tint(|| theme().accent)
    });
    // Trigger initial hover/press feel on Scene 2 counter.
    let _ = PointerButton::Primary;
    counter_ui.event(&InputEvent::AppearanceChanged(Appearance::new(
        ColorScheme::Dark,
        Some(Color::from_hex(0x7B61FF)),
    )));

    let mut bar_anims = [0.0; 5].map(Animated::new);
    let mut last_counter_step = usize::MAX;

    // Synthesize a warm ambient electronic soundtrack with ffmpeg in a temp path.
    let audio_path = format!("{out_path}.audio.m4a");
    let audio_expr = "0.10*sin(2*PI*130.81*t)+0.08*sin(2*PI*196.00*t)+0.06*sin(2*PI*246.94*t)+0.05*sin(2*PI*293.66*t)*(0.6+0.4*sin(2*PI*0.25*t))+0.04*sin(2*PI*523.25*t)*exp(-4*mod(t,0.4838))";
    let audio_status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("aevalsrc={audio_expr}:s=44100:d={TOTAL_SECS}"),
            "-af",
            "lowpass=f=1800,afade=t=in:st=0:d=1.2,afade=t=out:st=29.0:d=2.0",
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            &audio_path,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    let has_audio = audio_status.success();

    let mut ffmpeg_args = vec![
        "-y".to_string(),
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        "rgba".into(),
        "-s".into(),
        format!("{WIDTH}x{HEIGHT}"),
        "-r".into(),
        FPS.to_string(),
        "-i".into(),
        "-".into(),
    ];
    if has_audio {
        ffmpeg_args.extend(["-i".into(), audio_path.clone()]);
    }
    ffmpeg_args.extend([
        "-c:v".into(),
        "libx264".into(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-preset".into(),
        "medium".into(),
        "-crf".into(),
        "20".into(),
        "-movflags".into(),
        "+faststart".into(),
    ]);
    if has_audio {
        ffmpeg_args.extend(["-c:a".into(), "copy".into(), "-shortest".into()]);
    }
    ffmpeg_args.push(out_path.clone());

    let mut child = Command::new("ffmpeg")
        .args(&ffmpeg_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdin = child.stdin.take().expect("ffmpeg stdin");

    let mut renderer = OffscreenRenderer::new()?;
    let mut canvas = rugkit::Canvas::new();

    eprintln!("Rendering {TOTAL_FRAMES} frames ({WIDTH}x{HEIGHT} @ {FPS}fps) with rugkit GPU…");
    for frame_idx in 0..TOTAL_FRAMES {
        let t = f64::from(frame_idx) / f64::from(FPS);
        let mut p = canvas.begin_frame(SCALE);

        let light_mix = if (20.5..25.0).contains(&t) {
            let lt = t - 20.5;
            smoothstep(0.8, 1.9, lt) as f32 * (1.0 - smoothstep(3.3, 4.2, lt) as f32)
        } else {
            0.0
        };
        draw_backdrop(&mut p, t, light_mix);

        if t < 4.0 {
            draw_scene_intro(&mut p, t);
        } else if t < 8.5 {
            draw_scene_builder(
                &mut p,
                t - 4.0,
                &mut counter_ui,
                count,
                &mut last_counter_step,
            );
        } else if t < 14.5 {
            draw_scene_finance(&mut p, t - 8.5, &mut bar_anims);
        } else if t < 20.5 {
            draw_scene_studio(&mut p, t - 14.5);
        } else if t < 25.0 {
            let _ = draw_scene_theme_inspector(&mut p, t - 20.5);
        } else if t < 28.0 {
            draw_scene_perf(&mut p, t - 25.0);
        } else {
            draw_scene_outro(&mut p, t - 28.0);
        }

        let rgba = renderer.render_to_rgba(&canvas, WIDTH, HEIGHT, Color::from_hex(0x0D0B1A))?;
        stdin.write_all(&rgba)?;

        if frame_idx % 300 == 0 {
            eprintln!("  frame {frame_idx}/{TOTAL_FRAMES} ({t:.1}s)");
        }
    }
    drop(stdin);
    let status = child.wait()?;
    let _ = std::fs::remove_file(&audio_path);
    if !status.success() {
        return Err(format!("ffmpeg exited with {status}").into());
    }
    eprintln!("Done! Wrote {out_path}");
    Ok(())
}
