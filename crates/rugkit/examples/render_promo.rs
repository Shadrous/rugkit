//! Magnum Opus 1080p 60fps neon-noir launch motion video for `rugkit`.
//!
//! Rendered 100% natively on the GPU using rugkit's own [`OffscreenRenderer`],
//! [`Canvas`], [`Painter`], [`Spring`], [`IconName`], and a live [`Ui`], piped
//! directly into `ffmpeg` alongside a beat-locked 124 BPM synthwave soundtrack.
//!
//! Run:
//! ```bash
//! cargo run --release --example render_promo -- docs/demo/rugkit_launch.mp4
//! ```

use std::f64::consts::{PI, TAU};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

use rugkit::prelude::*;
use rugkit::{
    Animated, Canvas, FrameInfo, FrameStats, IconName, InputEvent, Motion, OffscreenRenderer,
    PointerButton, Transform, Ui,
};

const OUT_W: u32 = 1920;
const OUT_H: u32 = 1080;
const SCALE: f64 = 1.5; // 1280x720 logical -> 1920x1080 physical
const LOGICAL_W: f64 = OUT_W as f64 / SCALE;
const LOGICAL_H: f64 = OUT_H as f64 / SCALE;
const FPS: u32 = 60;
const DURATION_SECS: f64 = 42.0;
const TOTAL_FRAMES: u32 = (DURATION_SECS * FPS as f64) as u32;

/// Exact musical tempo of both the synthesized soundtrack and the GPU visuals.
const BPM: f64 = 124.0;
const BEAT_SECS: f64 = 60.0 / BPM;

// Disciplined neon-noir palette (high contrast, zero clutter).
const CYAN: u32 = 0x00E5FF;
const VIOLET: u32 = 0x8B5CF6;
const MAGENTA: u32 = 0xFF3B8D;
const MINT: u32 = 0x00F5A0;
const AMBER: u32 = 0xFFB020;

// ---- Easing & camera helpers ------------------------------------------------------------

fn clamp01(t: f64) -> f64 {
    t.clamp(0.0, 1.0)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = clamp01((x - e0) / (e1 - e0));
    t * t * (3.0 - 2.0 * t)
}

fn smootherstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = clamp01((x - e0) / (e1 - e0));
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn ease_out_cubic(t: f64) -> f64 {
    let u = 1.0 - clamp01(t);
    1.0 - u * u * u
}

fn ease_out_back(t: f64) -> f64 {
    let t = clamp01(t);
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

/// Returns `(beat_index, beat_frac, kick_pulse)` locked to 124 BPM.
fn music_clock(t: f64) -> (usize, f64, f64) {
    let beats = t / BEAT_SECS;
    let idx = beats.floor() as usize;
    let frac = beats.fract();
    let pulse = (1.0 - frac).powf(3.2);
    (idx, frac, pulse)
}

/// Applies a virtual cinema camera focusing on `focus` at `zoom`, placing it at screen center.
fn with_camera<R>(
    p: &mut Painter<'_>,
    focus: Point,
    zoom: f64,
    f: impl FnOnce(&mut Painter<'_>) -> R,
) -> R {
    let center = Point::new(LOGICAL_W * 0.5, LOGICAL_H * 0.5);
    let tx = center.x - focus.x * zoom;
    let ty = center.y - focus.y * zoom;
    let xform =
        Transform::scale_about(zoom, Point::ZERO).then(Transform::translate(Vec2::new(tx, ty)));
    p.with_transform(xform, f)
}

// ---- Core drawing primitives ------------------------------------------------------------

fn draw_centered_text(p: &mut Painter<'_>, s: &str, cx: f64, y: f64, style: &TextStyle) {
    let m = p.measure_text(s, None, style);
    p.text(s, Point::new(cx - m.width * 0.5, y), style);
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

fn draw_pill(
    p: &mut Painter<'_>,
    label: &str,
    cx: f64,
    y: f64,
    bg: Color,
    fg: Color,
    border: Color,
) {
    let style = TextStyle::new(13.5).weight(FontWeight::BOLD).color(fg);
    let m = p.measure_text(label, None, &style);
    let w = m.width + 26.0;
    let h = 30.0;
    let rect = Rect::from_xywh(cx - w * 0.5, y, w, h);
    p.fill_rect(rect, 999.0, bg);
    if border.a > 0.01 {
        p.stroke_rect(rect, 999.0, 1.1, border);
    }
    p.text(label, Point::new(cx - m.width * 0.5, y + 6.5), &style);
}

/// Crisp vector pointer cursor with neon halo and expanding click ripple.
fn draw_cursor(p: &mut Painter<'_>, pos: Point, click_age: Option<f64>, accent: Color, alpha: f32) {
    if alpha <= 0.01 {
        return;
    }
    if let Some(age) = click_age
        && (0.0..0.48).contains(&age)
    {
        let u = age / 0.48;
        let r = 8.0 + ease_out_cubic(u) * 30.0;
        let ra = ((1.0 - u) as f32) * 0.85 * alpha;
        p.stroke_rect(
            Rect::from_xywh(pos.x - r, pos.y - r, r * 2.0, r * 2.0),
            999.0,
            2.2,
            accent.with_alpha(ra),
        );
        p.fill_circle(pos, 5.0 * (1.0 - u), accent.with_alpha(ra * 0.6));
    }

    // Soft glow under tip
    p.shadow(
        Rect::from_xywh(pos.x - 4.0, pos.y - 4.0, 16.0, 18.0),
        8.0,
        &Shadow::new(accent.with_alpha(0.55 * alpha), Vec2::new(0.0, 3.0), 12.0),
    );

    let mut arrow = Path::new();
    arrow.move_to(pos);
    arrow.line_to(Point::new(pos.x, pos.y + 19.0));
    arrow.line_to(Point::new(pos.x + 5.2, pos.y + 14.8));
    arrow.line_to(Point::new(pos.x + 9.2, pos.y + 21.5));
    arrow.line_to(Point::new(pos.x + 12.2, pos.y + 20.0));
    arrow.line_to(Point::new(pos.x + 8.3, pos.y + 13.2));
    arrow.line_to(Point::new(pos.x + 14.4, pos.y + 12.6));
    arrow.close();
    p.fill_path(&arrow, Color::WHITE.with_alpha(alpha));
    p.stroke_path(&arrow, 1.4, Color::from_hex(0x090814).with_alpha(alpha));
}

/// Editorial lower-left or bottom caption overlay (clean, restrained, zero clutter).
fn draw_editorial_hud(
    p: &mut Painter<'_>,
    eyebrow: &str,
    headline: &str,
    accent: Color,
    alpha: f32,
    light_mix: f32,
) {
    if alpha <= 0.01 {
        return;
    }
    let y_bar = 650.0;
    let ink = Color::from_hex(0xF6F5FF).mix(Color::from_hex(0x16142A), light_mix);
    let sub = accent.mix(Color::from_hex(0x6D4AFF), light_mix * 0.5);

    // Subtle frosted pill bar at bottom-left
    let h_style = TextStyle::new(20.0)
        .weight(FontWeight::BOLD)
        .letter_spacing(-0.4)
        .color(ink.with_alpha(alpha));
    let e_style = TextStyle::new(11.0)
        .weight(FontWeight::BLACK)
        .letter_spacing(1.6)
        .color(sub.with_alpha(alpha));
    let hw = p.measure_text(headline, None, &h_style).width;
    let ew = p.measure_text(eyebrow, None, &e_style).width;
    let pill_w = hw.max(ew) + 46.0;
    let pill = Rect::from_xywh(42.0, y_bar - 14.0, pill_w, 56.0);
    let pill_bg = Color::rgba(0.05, 0.04, 0.11, 0.80)
        .mix(Color::rgba(1.0, 0.98, 0.95, 0.86), light_mix)
        .with_alpha(0.86 * alpha);
    p.fill_rect(pill, 16.0, pill_bg);
    p.stroke_rect(pill, 16.0, 1.0, sub.with_alpha(0.45 * alpha));
    // Neon accent bar on left edge
    p.fill_rect(
        Rect::from_xywh(pill.x0 + 12.0, pill.y0 + 12.0, 3.5, 32.0),
        2.0,
        sub.with_alpha(alpha),
    );
    p.text(eyebrow, Point::new(pill.x0 + 26.0, pill.y0 + 9.0), &e_style);
    p.text(
        headline,
        Point::new(pill.x0 + 26.0, pill.y0 + 25.0),
        &h_style,
    );
}

// ---- Neon-Noir Cinema Scenery (Atmosphere + Perspective Floor + Parallax Motes) ---------

fn draw_neon_scenery(
    p: &mut Painter<'_>,
    t: f64,
    light_mix: f32,
    parallax: Vec2,
    accent_a: Color,
    accent_b: Color,
) {
    let (_, _, pulse) = music_clock(t);
    let dark_a = 1.0 - light_mix;

    // 1. Deep obsidian sky -> horizon -> floor gradient
    let sky_top = Color::from_hex(0x04030A).mix(Color::from_hex(0xFFF6EC), light_mix);
    let sky_mid = Color::from_hex(0x0B081D).mix(Color::from_hex(0xFCE8D5), light_mix);
    let floor_bot = Color::from_hex(0x05030D).mix(Color::from_hex(0xF5D9BF), light_mix);

    let horizon_y = 468.0 + parallax.y * 0.18;
    p.fill_rect(
        Rect::from_xywh(0.0, 0.0, LOGICAL_W, horizon_y),
        0.0,
        Brush::linear(
            Point::ZERO,
            Point::new(0.0, horizon_y),
            [(0.0, sky_top), (1.0, sky_mid)],
        ),
    );
    p.fill_rect(
        Rect::from_xywh(0.0, horizon_y, LOGICAL_W, LOGICAL_H - horizon_y),
        0.0,
        Brush::linear(
            Point::new(0.0, horizon_y),
            Point::new(0.0, LOGICAL_H),
            [(0.0, sky_mid), (1.0, floor_bot)],
        ),
    );

    // 2. Atmospheric volumetric color clouds (beat-reactive, restrained)
    let cloud_a = (0.16 + 0.06 * pulse as f32) * dark_a;
    if cloud_a > 0.01 {
        let c1 = Point::new(
            300.0 + (t * 0.25).sin() * 55.0 + parallax.x * 0.25,
            230.0 + (t * 0.2).cos() * 30.0 + parallax.y * 0.2,
        );
        let c2 = Point::new(
            980.0 + (t * 0.22).cos() * 65.0 + parallax.x * 0.25,
            410.0 + (t * 0.18).sin() * 35.0 + parallax.y * 0.2,
        );
        let c3 = Point::new(
            640.0 + (t * 0.16).sin() * 90.0 + parallax.x * 0.15,
            horizon_y - 30.0,
        );
        p.shadow(
            Rect::from_xywh(c1.x - 170.0, c1.y - 120.0, 340.0, 240.0),
            120.0,
            &Shadow::new(accent_a.with_alpha(cloud_a), Vec2::ZERO, 130.0),
        );
        p.shadow(
            Rect::from_xywh(c2.x - 180.0, c2.y - 120.0, 360.0, 240.0),
            120.0,
            &Shadow::new(accent_b.with_alpha(cloud_a * 0.9), Vec2::ZERO, 130.0),
        );
        p.shadow(
            Rect::from_xywh(c3.x - 240.0, c3.y - 40.0, 480.0, 80.0),
            40.0,
            &Shadow::new(
                Color::from_hex(VIOLET).with_alpha(cloud_a * 0.85),
                Vec2::ZERO,
                100.0,
            ),
        );
    }

    // 3. Perspective architectural neon floor grid receding to vanishing point
    let grid_a = 0.16 * dark_a + 0.04 * light_mix;
    let vp_x = LOGICAL_W * 0.5 + parallax.x * 0.35;
    let vp_y = horizon_y;
    let grid_col = accent_a.mix(Color::from_hex(0x7B61FF), 0.45);

    // Radial perspective lines
    for i in -9i32..=9 {
        let spread = i as f64 * 125.0;
        let bx = vp_x + spread * 2.15;
        let dist_fade = (1.0f32 - (i.abs() as f32 / 10.5)).max(0.15);
        p.stroke_line(
            Point::new(vp_x + spread * 0.08, vp_y),
            Point::new(bx, LOGICAL_H),
            1.0,
            grid_col.with_alpha(grid_a * dist_fade),
        );
    }

    // Forward-scrolling horizontal depth lines (exponential perspective spacing)
    let scroll = (t * 0.72).fract();
    for k in 0..10 {
        let z = (k as f64 + scroll) / 10.0;
        let depth = z * z; // quadratic foreshortening
        let y = vp_y + depth * (LOGICAL_H - vp_y);
        let la = (depth as f32).powf(0.75) * grid_a * 1.15;
        p.stroke_line(
            Point::new(0.0, y),
            Point::new(LOGICAL_W, y),
            1.0,
            grid_col.with_alpha(la),
        );
    }

    // 4. Razor-sharp glowing horizon light ribbon
    if dark_a > 0.05 {
        let rib_a = (0.38 + 0.25 * pulse as f32) * dark_a;
        p.shadow(
            Rect::from_xywh(120.0, vp_y - 2.0, LOGICAL_W - 240.0, 4.0),
            2.0,
            &Shadow::new(accent_a.with_alpha(rib_a * 0.7), Vec2::ZERO, 24.0),
        );
        p.fill_rect(
            Rect::from_xywh(0.0, vp_y - 1.0, LOGICAL_W, 2.0),
            0.0,
            Brush::linear(
                Point::new(0.0, vp_y),
                Point::new(LOGICAL_W, vp_y),
                [
                    (0.0, Color::TRANSPARENT),
                    (0.22, accent_a.with_alpha(rib_a)),
                    (0.50, Color::WHITE.with_alpha(rib_a * 0.95)),
                    (0.78, accent_b.with_alpha(rib_a)),
                    (1.0, Color::TRANSPARENT),
                ],
            ),
        );
    }

    // 5. Drifting parallax light motes
    if dark_a > 0.08 {
        for i in 0..18 {
            let fi = i as f64;
            let seed_x = ((fi * 173.3 + 41.0) % 1.0) * LOGICAL_W;
            let speed = 12.0 + (i % 4) as f64 * 7.0;
            let y = LOGICAL_H - ((fi * 97.0 + t * speed) % (LOGICAL_H + 40.0)) + 20.0;
            let x =
                (seed_x + (t * 0.4 + fi).sin() * 24.0 + parallax.x * (0.1 + (i % 3) as f64 * 0.08))
                    .rem_euclid(LOGICAL_W);
            let r = 1.1 + (i % 3) as f64 * 0.65;
            let col = if i % 3 == 0 {
                accent_a
            } else if i % 3 == 1 {
                accent_b
            } else {
                Color::WHITE
            };
            let twinkle = 0.25 + 0.35 * ((t * 2.1 + fi * 1.7).sin() * 0.5 + 0.5);
            p.fill_circle(Point::new(x, y), r, col.with_alpha(twinkle as f32 * dark_a));
        }
    }
}

/// Floating macOS glass-rimmed workstation window with neon ambient shadow.
fn draw_neon_window(p: &mut Painter<'_>, frame: Rect, title: &str, accent: Color, light_mix: f32) {
    let bg = Color::from_hex(0x0F0D1D).mix(Color::from_hex(0xFFF6EC), light_mix);
    let border = accent
        .with_alpha(0.32)
        .mix(Color::rgba(0.12, 0.10, 0.24, 0.16), light_mix);

    // Layered ambient + neon-tinted shadow
    p.shadow(
        frame,
        24.0,
        &Shadow::new(
            Color::rgba(0.0, 0.0, 0.0, 0.62 - 0.30 * light_mix),
            Vec2::new(0.0, 22.0),
            44.0,
        ),
    );
    if light_mix < 0.7 {
        p.shadow(
            frame,
            24.0,
            &Shadow::new(
                accent.with_alpha(0.22 * (1.0 - light_mix)),
                Vec2::new(0.0, 2.0),
                28.0,
            ),
        );
    }

    p.fill_rect(frame, 24.0, bg);
    p.stroke_rect(frame, 24.0, 1.3, border);

    // Top specular glass highlight edge
    p.stroke_line(
        Point::new(frame.x0 + 28.0, frame.y0 + 1.0),
        Point::new(frame.x1 - 28.0, frame.y0 + 1.0),
        1.2,
        Color::WHITE.with_alpha(0.24 * (1.0 - light_mix) + 0.55 * light_mix),
    );

    // macOS traffic lights
    let cy = frame.y0 + 20.0;
    for (i, hex) in [0xFF5F57u32, 0xFEBC2E, 0x28C840].into_iter().enumerate() {
        p.fill_circle(
            Point::new(frame.x0 + 22.0 + i as f64 * 17.0, cy),
            5.2,
            Color::from_hex(hex),
        );
    }

    let title_col = Color::from_hex(0x9E98BF).mix(Color::from_hex(0x6B6687), light_mix);
    draw_centered_text(
        p,
        title,
        frame.center().x,
        frame.y0 + 11.5,
        &TextStyle::new(12.5)
            .weight(FontWeight::SEMIBOLD)
            .color(title_col),
    );
}

// ---- ACT I: NEON HORIZON & KINETIC INTRO (0.0s .. 5.0s) ---------------------------------

fn draw_act_intro(p: &mut Painter<'_>, lt: f64, global_t: f64) {
    let (_, _, pulse) = music_clock(global_t);
    let fade_in = smoothstep(0.0, 0.55, lt) as f32;
    let fade_out = (1.0 - smoothstep(4.25, 5.0, lt)) as f32;
    let alpha = fade_in * fade_out;

    // Dramatic zoom-in exit at the end of Act I
    let exit_zoom = 1.0 + smootherstep(4.1, 5.0, lt) * 0.42;

    with_camera(p, Point::new(LOGICAL_W * 0.5, 320.0), exit_zoom, |p| {
        // Luminous soundwave ribbon slicing across the center
        let wave_reveal = ease_out_cubic(lt / 1.1);
        let mut wave = Path::new();
        let half_w = 540.0 * wave_reveal;
        for s in 0..96 {
            let u = s as f64 / 95.0;
            let x = LOGICAL_W * 0.5 - half_w + u * half_w * 2.0;
            let env = (u * PI).sin().powf(1.6);
            let y = 315.0
                + (u * TAU * 3.5 - lt * 3.8).sin() * (22.0 + 16.0 * pulse) * env
                + (u * TAU * 7.0 + lt * 5.2).cos() * 7.0 * env;
            if s == 0 {
                wave.move_to(Point::new(x, y));
            } else {
                wave.line_to(Point::new(x, y));
            }
        }
        p.stroke_path(&wave, 10.0, Color::from_hex(CYAN).with_alpha(0.18 * alpha));
        p.stroke_path(
            &wave,
            2.2,
            Brush::linear(
                Point::new(120.0, 315.0),
                Point::new(LOGICAL_W - 120.0, 315.0),
                [
                    (0.0, Color::from_hex(VIOLET).with_alpha(0.0)),
                    (0.25, Color::from_hex(CYAN).with_alpha(0.95 * alpha)),
                    (0.5, Color::WHITE.with_alpha(alpha)),
                    (0.75, Color::from_hex(MAGENTA).with_alpha(0.95 * alpha)),
                    (1.0, Color::from_hex(VIOLET).with_alpha(0.0)),
                ],
            ),
        );

        // Eyebrow
        let eye_t = ease_out_cubic((lt - 0.35) / 0.55);
        draw_centered_text(
            p,
            "PURE RUST  ·  VELLO METAL GPU  ·  120FPS SPRINGS",
            LOGICAL_W * 0.5,
            174.0 + (1.0 - eye_t) * 14.0,
            &TextStyle::new(12.5)
                .weight(FontWeight::BLACK)
                .letter_spacing(3.2)
                .color(Color::from_hex(CYAN).with_alpha((eye_t as f32) * alpha)),
        );

        // Kinetic wordmark with layered chromatic neon bloom
        let title_scale = 0.78 + 0.22 * ease_out_back((lt - 0.15) / 0.85);
        p.with_transform(
            Transform::scale_about(title_scale, Point::new(LOGICAL_W * 0.5, 265.0)),
            |p| {
                p.shadow(
                    Rect::from_xywh(LOGICAL_W * 0.5 - 210.0, 218.0, 420.0, 96.0),
                    48.0,
                    &Shadow::new(
                        Color::from_hex(VIOLET).with_alpha((0.42 + 0.14 * pulse as f32) * alpha),
                        Vec2::new(-18.0, 0.0),
                        56.0,
                    ),
                );
                p.shadow(
                    Rect::from_xywh(LOGICAL_W * 0.5 - 210.0, 218.0, 420.0, 96.0),
                    48.0,
                    &Shadow::new(
                        Color::from_hex(CYAN).with_alpha((0.28 + 0.12 * pulse as f32) * alpha),
                        Vec2::new(22.0, 4.0),
                        52.0,
                    ),
                );
                draw_centered_text(
                    p,
                    "rugkit",
                    LOGICAL_W * 0.5,
                    195.0,
                    &TextStyle::new(122.0)
                        .weight(FontWeight::BLACK)
                        .letter_spacing(-5.0)
                        .color(Color::WHITE.with_alpha(alpha)),
                );
            },
        );

        // Editorial tagline
        let sub_t = ease_out_cubic((lt - 0.85) / 0.65);
        draw_centered_text(
            p,
            "The one framework that cares about visuals.",
            LOGICAL_W * 0.5,
            378.0 + (1.0 - sub_t) * 20.0,
            &TextStyle::new(28.0)
                .weight(FontWeight::SEMIBOLD)
                .letter_spacing(-0.5)
                .color(Color::from_hex(0xD8D4F5).with_alpha((sub_t as f32) * alpha)),
        );
    });
}

// ---- ACT II: PLAIN RUST -> LIVING UI WITH CURSOR & SPRINGS (5.0s .. 11.0s) --------------

fn draw_act_builder(
    p: &mut Painter<'_>,
    lt: f64,
    ui: &mut Ui,
    count: Signal<i32>,
    last_step: &mut usize,
) {
    let enter = smootherstep(0.0, 0.75, lt);
    let fade = (smoothstep(0.0, 0.4, lt) * (1.0 - smoothstep(5.4, 6.0, lt))) as f32;

    // Subtle camera drift: starts focused slightly on code, glides right to the live springs!
    let cam_x = lerp(575.0, 695.0, smootherstep(0.8, 4.2, lt));
    let cam_y = lerp(330.0, 336.0, smootherstep(0.8, 4.2, lt));
    let zoom = lerp(1.12, 1.02, smootherstep(0.0, 1.6, lt));

    // Choreographed cursor clicks on the live Counter Ui (+, +, +, -, Reset, +)
    let click_times = [1.4, 2.0, 2.55, 3.25, 4.0, 4.65];
    let step_idx = click_times.iter().filter(|&&ct| lt >= ct).count();
    if step_idx != *last_step {
        *last_step = step_idx;
        const VALUES: [i32; 7] = [0, 1, 3, 7, 5, 0, 9];
        count.set(VALUES[step_idx.min(VALUES.len() - 1)]);
    }

    with_camera(p, Point::new(cam_x, cam_y), zoom, |p| {
        // Left: Glass-rimmed Plain Rust code card
        let code_rect = Rect::from_xywh(74.0 - (1.0 - enter) * 45.0, 82.0, 540.0, 488.0);
        p.shadow(
            code_rect,
            24.0,
            &Shadow::new(
                Color::rgba(0.0, 0.0, 0.0, 0.55 * fade),
                Vec2::new(0.0, 18.0),
                36.0,
            ),
        );
        p.fill_rect(code_rect, 24.0, Color::rgba(0.06, 0.05, 0.13, 0.94 * fade));
        p.stroke_rect(
            code_rect,
            24.0,
            1.2,
            Color::from_hex(CYAN).with_alpha(0.32 * fade),
        );
        p.stroke_line(
            Point::new(code_rect.x0 + 24.0, code_rect.y0 + 1.0),
            Point::new(code_rect.x1 - 24.0, code_rect.y0 + 1.0),
            1.2,
            Color::WHITE.with_alpha(0.22 * fade),
        );

        p.text(
            "src/main.rs  ·  Zero Proc Macros",
            Point::new(code_rect.x0 + 24.0, code_rect.y0 + 20.0),
            &TextStyle::new(13.5)
                .weight(FontWeight::BOLD)
                .color(Color::from_hex(CYAN).with_alpha(fade)),
        );

        let lines: [(&str, u32); 17] = [
            ("let count = signal(0);", MINT),
            ("", 0xFFFFFF),
            ("column((", 0xF5F3FF),
            ("  text(move || count.get().to_string())", 0xA89EFF),
            ("    .size(96.0)", 0xD8D4F5),
            ("    .weight(FontWeight::BLACK)", 0xD8D4F5),
            ("    .color(tone),", 0xD8D4F5),
            ("  row((", 0xF5F3FF),
            ("    button(\"-\").secondary()", AMBER),
            (
                "      .on_click(move || count.update(|c| *c -= 1)),",
                0xB8B2D8,
            ),
            ("    button(\"+\").primary()", CYAN),
            (
                "      .on_click(move || count.update(|c| *c += 1)),",
                0xB8B2D8,
            ),
            ("  )).gap(12.0),", 0xF5F3FF),
            ("))", 0xF5F3FF),
            (".padding((30.0, 42.0))", 0xD8D4F5),
            (".radius(34.0)", 0xD8D4F5),
            (".elevation(10.0)", 0xD8D4F5),
        ];

        // Glowing active-line highlight sweeping down the builder chain
        let active_line = ((lt * 3.0) as usize) % lines.len();
        let hl_y = code_rect.y0 + 54.0 + active_line as f64 * 24.5;
        p.fill_rect(
            Rect::from_xywh(
                code_rect.x0 + 12.0,
                hl_y - 3.0,
                code_rect.width() - 24.0,
                23.0,
            ),
            6.0,
            Color::from_hex(CYAN).with_alpha(0.09 * fade),
        );

        for (i, (line, hex)) in lines.into_iter().enumerate() {
            let st = TextStyle::new(13.8)
                .family(FontFamily::Monospace)
                .weight(FontWeight::MEDIUM)
                .color(Color::from_hex(hex).with_alpha(fade));
            p.text(
                line,
                Point::new(code_rect.x0 + 24.0, code_rect.y0 + 54.0 + i as f64 * 24.5),
                &st,
            );
        }

        // Right: Live rugkit::Ui Counter Window!
        let win = Rect::from_xywh(646.0 + (1.0 - enter) * 45.0, 82.0, 560.0, 488.0);
        draw_neon_window(
            p,
            win,
            "rugkit · live spring physics",
            Color::from_hex(VIOLET),
            0.0,
        );

        let inner = Rect::from_xywh(win.x0, win.y0 + 38.0, win.width(), win.height() - 38.0);
        let info = FrameInfo {
            size: inner.size(),
            scale: SCALE,
            time: Duration::from_secs_f64(lt),
            dt: Duration::from_secs_f64(1.0 / f64::from(FPS)),
            stats: FrameStats::default(),
            reduce_motion: false,
            appearance: Appearance::new(ColorScheme::Dark, Some(Color::from_hex(CYAN))),
        };
        p.with_clip(inner, 20.0, |p| {
            p.with_transform(Transform::translate(Vec2::new(inner.x0, inner.y0)), |p| {
                ui.draw(p, &info);
            });
        });

        // Living cursor clicking the +, -, and Reset buttons
        let plus_pt = Point::new(win.center().x + 92.0, win.y0 + 382.0);
        let minus_pt = Point::new(win.center().x - 94.0, win.y0 + 382.0);
        let reset_pt = Point::new(win.center().x, win.y0 + 382.0);
        let cursor_pos = if lt < 2.9 {
            let u = smootherstep(0.5, 1.35, lt);
            Point::new(
                lerp(win.x0 + 80.0, plus_pt.x, u),
                lerp(win.y0 + 140.0, plus_pt.y, u),
            )
        } else if lt < 3.65 {
            let u = smootherstep(2.9, 3.22, lt);
            Point::new(
                lerp(plus_pt.x, minus_pt.x, u),
                lerp(plus_pt.y, minus_pt.y, u),
            )
        } else if lt < 4.35 {
            let u = smootherstep(3.65, 3.96, lt);
            Point::new(
                lerp(minus_pt.x, reset_pt.x, u),
                lerp(minus_pt.y, reset_pt.y, u),
            )
        } else {
            let u = smootherstep(4.35, 4.62, lt);
            Point::new(
                lerp(reset_pt.x, plus_pt.x, u),
                lerp(reset_pt.y, plus_pt.y, u),
            )
        };
        let click_age = click_times
            .iter()
            .filter(|&&ct| lt >= ct && lt - ct < 0.48)
            .map(|&ct| lt - ct)
            .next_back();
        draw_cursor(p, cursor_pos, click_age, Color::from_hex(CYAN), fade);
    });

    draw_editorial_hud(
        p,
        "01 / PLAIN RUST BUILDERS",
        "Fine-grained signals. Implicit interruptible springs.",
        Color::from_hex(CYAN),
        fade,
        0.0,
    );
}

// ---- ACT III: FLAGSHIP #1 — MINTY MULTI-SCREEN TOUR (11.0s .. 21.5s) --------------------
// 3 real interactive sections:
//   0.0..3.6s : Overview (Macro close-up on Spending Bars + Cashflow + Hover Tooltip)
//   3.6..7.1s : Cursor clicks "Transactions" -> 100k-row Virtualised Table + Search + Multi-Select
//   7.1..10.5s: Cursor clicks "Budgets" -> Interactive Budget Sliders + Import JSON Modal

fn draw_act_minty(p: &mut Painter<'_>, lt: f64, bar_anims: &mut [Animated<f64>; 5]) {
    let fade = (smoothstep(0.0, 0.4, lt) * (1.0 - smoothstep(9.9, 10.5, lt))) as f32;
    let dt = 1.0 / f64::from(FPS);

    // Determine active sidebar tab & smooth pill position
    let tab_idx: usize = if lt < 3.6 {
        0
    } else if lt < 7.1 {
        1
    } else {
        2
    };
    let pill_slot = smootherstep(3.45, 3.78, lt) + smootherstep(6.95, 7.28, lt);

    // Virtual cinema camera choreography across the 3 Minty sections
    let (cam_x, cam_y, zoom) = if lt < 3.4 {
        // Start tight on the spending chart + hover tooltip, then pull out to full window
        let u = smootherstep(0.6, 2.9, lt);
        (
            lerp(560.0, 640.0, u),
            lerp(310.0, 336.0, u),
            lerp(1.34, 1.02, u),
        )
    } else if lt < 7.0 {
        // Slight zoom onto the 100k-row Transactions table & search bar
        let u = smootherstep(3.8, 5.8, lt);
        (
            lerp(640.0, 690.0, u),
            lerp(336.0, 355.0, u),
            lerp(1.02, 1.15, u),
        )
    } else {
        // Pull back cleanly as Budgets + Import JSON Modal pop into view
        let u = smootherstep(7.0, 8.6, lt);
        (
            lerp(690.0, 640.0, u),
            lerp(355.0, 332.0, u),
            lerp(1.15, 1.01, u),
        )
    };

    let win = Rect::from_xywh(100.0, 44.0, 1080.0, 566.0);
    let surface = Color::from_hex(0x161328);
    let border = Color::rgba(0.95, 0.94, 1.0, 0.10);
    let soft = Color::from_hex(0x9E98BF);
    let violet = Color::from_hex(VIOLET);
    let mint = Color::from_hex(MINT);
    let cyan = Color::from_hex(CYAN);
    let magenta = Color::from_hex(MAGENTA);

    with_camera(p, Point::new(cam_x, cam_y), zoom, |p| {
        draw_neon_window(p, win, "Minty · Personal Finance Flagship", violet, 0.0);

        // ---- Persistent Sidebar ----
        let sb = Rect::from_xywh(win.x0 + 16.0, win.y0 + 44.0, 190.0, win.height() - 60.0);
        p.fill_rect(sb, 18.0, Color::from_hex(0x131024));
        p.stroke_rect(sb, 18.0, 1.0, border);
        draw_icon_at(
            p,
            IconName::Wallet,
            Point::new(sb.x0 + 28.0, sb.y0 + 28.0),
            22.0,
            violet,
        );
        p.text(
            "Minty",
            Point::new(sb.x0 + 48.0, sb.y0 + 16.0),
            &TextStyle::new(20.0)
                .weight(FontWeight::BLACK)
                .color(Color::WHITE),
        );

        // Sliding glowing active tab pill
        let active_y = sb.y0 + 66.0 + pill_slot * 44.0;
        let pill_r = Rect::from_xywh(sb.x0 + 12.0, active_y, sb.width() - 24.0, 36.0);
        p.shadow(
            pill_r,
            12.0,
            &Shadow::new(violet.with_alpha(0.42), Vec2::new(0.0, 4.0), 14.0),
        );
        p.fill_rect(
            pill_r,
            12.0,
            Brush::linear(
                Point::new(pill_r.x0, pill_r.y0),
                Point::new(pill_r.x1, pill_r.y0),
                [(0.0, violet), (1.0, Color::from_hex(0x6D4AFF))],
            ),
        );

        let nav = [
            (IconName::Home, "Overview"),
            (IconName::CreditCard, "Transactions"),
            (IconName::PieChart, "Budgets"),
            (IconName::Settings, "Settings"),
        ];
        for (i, (ic, lbl)) in nav.into_iter().enumerate() {
            let ny = sb.y0 + 66.0 + i as f64 * 44.0;
            let active = i == tab_idx;
            let fg = if active { Color::WHITE } else { soft };
            draw_icon_at(p, ic, Point::new(sb.x0 + 30.0, ny + 18.0), 16.0, fg);
            p.text(
                lbl,
                Point::new(sb.x0 + 48.0, ny + 9.5),
                &TextStyle::new(14.0).weight(FontWeight::BOLD).color(fg),
            );
        }

        // Bottom sidebar summary badge
        let sync_r = Rect::from_xywh(sb.x0 + 12.0, sb.y1 - 64.0, sb.width() - 24.0, 50.0);
        p.fill_rect(sync_r, 12.0, Color::rgba(0.0, 0.96, 0.63, 0.08));
        p.stroke_rect(sync_r, 12.0, 1.0, mint.with_alpha(0.25));
        p.fill_circle(Point::new(sync_r.x0 + 16.0, sync_r.y0 + 18.0), 4.0, mint);
        p.text(
            "Local JSON",
            Point::new(sync_r.x0 + 28.0, sync_r.y0 + 10.0),
            &TextStyle::new(12.0)
                .weight(FontWeight::BOLD)
                .color(Color::WHITE),
        );
        p.text(
            "100,000 rows",
            Point::new(sync_r.x0 + 28.0, sync_r.y0 + 27.0),
            &TextStyle::new(11.0).color(mint),
        );

        // ---- Main Content Viewport (clipped to main pane) ----
        let main_x = sb.x1 + 16.0;
        let main_w = win.x1 - main_x - 16.0;
        let main_r = Rect::from_xywh(main_x, win.y0 + 44.0, main_w, win.height() - 60.0);

        p.with_clip(main_r, 16.0, |p| {
            // ===== SECTION 1: OVERVIEW (KPIs + Animated Charts + Hover Tooltip) =====
            if lt < 3.95 {
                let s1_a = (1.0 - smoothstep(3.45, 3.85, lt)) as f32;
                let slide_x = -smootherstep(3.45, 3.85, lt) * 40.0;
                p.with_opacity(s1_a, main_r, |p| {
                    p.with_transform(Transform::translate(Vec2::new(slide_x, 0.0)), |p| {
                        // KPI row
                        let kpi_w = (main_w - 28.0) / 3.0;
                        let kpis = [
                            (
                                "Net Balance",
                                "$118,314.24",
                                "+12.4%",
                                violet,
                                IconName::Wallet,
                            ),
                            (
                                "Monthly Income",
                                "$14,820.00",
                                "+4.2%",
                                mint,
                                IconName::TrendingUp,
                            ),
                            (
                                "Monthly Spend",
                                "$3,418.72",
                                "-8.1%",
                                magenta,
                                IconName::CreditCard,
                            ),
                        ];
                        for (i, (lbl, val, delta, col, ic)) in kpis.into_iter().enumerate() {
                            let kr = Rect::from_xywh(
                                main_x + i as f64 * (kpi_w + 14.0),
                                main_r.y0,
                                kpi_w,
                                92.0,
                            );
                            p.fill_rect(kr, 18.0, surface);
                            p.stroke_rect(kr, 18.0, 1.0, border);
                            p.text(
                                lbl,
                                Point::new(kr.x0 + 18.0, kr.y0 + 14.0),
                                &TextStyle::new(12.5).color(soft),
                            );
                            draw_icon_at(p, ic, Point::new(kr.x1 - 24.0, kr.y0 + 22.0), 17.0, col);
                            p.text(
                                val,
                                Point::new(kr.x0 + 18.0, kr.y0 + 40.0),
                                &TextStyle::new(24.0)
                                    .weight(FontWeight::BLACK)
                                    .color(Color::WHITE),
                            );
                            p.text(
                                delta,
                                Point::new(kr.x1 - 66.0, kr.y0 + 62.0),
                                &TextStyle::new(12.0).weight(FontWeight::BOLD).color(col),
                            );
                        }

                        // Spending by Category Canvas
                        let bar_card =
                            Rect::from_xywh(main_x, main_r.y0 + 106.0, main_w * 0.54, 240.0);
                        p.fill_rect(bar_card, 20.0, surface);
                        p.stroke_rect(bar_card, 20.0, 1.0, border);
                        p.text(
                            "Spending by Category",
                            Point::new(bar_card.x0 + 20.0, bar_card.y0 + 16.0),
                            &TextStyle::new(15.0)
                                .weight(FontWeight::BOLD)
                                .color(Color::WHITE),
                        );

                        let targets = [0.76, 0.52, 0.44, 0.64, 0.88];
                        let cats = [
                            ("Groceries", "$1,240", VIOLET),
                            ("Dining", "$680", AMBER),
                            ("Tech", "$540", CYAN),
                            ("Travel", "$820", MAGENTA),
                            ("Health", "$1,410", MINT),
                        ];
                        let hovered_bar = if (0.7..2.0).contains(&lt) {
                            Some(0usize)
                        } else if (2.0..3.4).contains(&lt) {
                            Some(4usize)
                        } else {
                            None
                        };

                        let slot_w = (bar_card.width() - 48.0) / 5.0;
                        for (i, ((name, amt, hex), &tgt)) in
                            cats.iter().zip(targets.iter()).enumerate()
                        {
                            let boost = if hovered_bar == Some(i) { 0.08 } else { 0.0 };
                            bar_anims[i].set_target(tgt + boost);
                            bar_anims[i].step(dt, Motion::Spring(Spring::BOUNCY), false);
                            let h = (bar_anims[i].value() * 145.0).max(10.0);
                            let bx = bar_card.x0 + 26.0 + i as f64 * slot_w + slot_w * 0.14;
                            let bw = slot_w * 0.66;
                            let by = bar_card.y1 - 34.0 - h;
                            let col = Color::from_hex(*hex);
                            let r = Rect::from_xywh(bx, by, bw, h);
                            if hovered_bar == Some(i) {
                                p.shadow(
                                    r,
                                    11.0,
                                    &Shadow::new(col.with_alpha(0.55), Vec2::ZERO, 18.0),
                                );
                            }
                            p.fill_rect(
                                r,
                                11.0,
                                Brush::linear(
                                    Point::new(bx, by),
                                    Point::new(bx, by + h),
                                    [(0.0, col), (1.0, col.with_alpha(0.32))],
                                ),
                            );
                            if hovered_bar == Some(i) {
                                p.stroke_rect(r, 11.0, 1.6, Color::WHITE.with_alpha(0.85));
                                // Floating crisp hover tooltip!
                                let tip = Rect::from_xywh(
                                    (bx - 24.0).max(bar_card.x0 + 12.0),
                                    by - 40.0,
                                    114.0,
                                    30.0,
                                );
                                p.fill_rect(tip, 8.0, Color::from_hex(0x0B0917));
                                p.stroke_rect(tip, 8.0, 1.1, col);
                                p.text(
                                    &format!("{name} · {amt}"),
                                    Point::new(tip.x0 + 8.0, tip.y0 + 7.5),
                                    &TextStyle::new(11.0)
                                        .weight(FontWeight::BOLD)
                                        .color(Color::WHITE),
                                );
                            }
                            draw_centered_text(
                                p,
                                name,
                                bx + bw * 0.5,
                                bar_card.y1 - 24.0,
                                &TextStyle::new(11.0).color(soft),
                            );
                        }

                        // 6-Month Cashflow Curve Canvas
                        let line_card = Rect::from_xywh(
                            bar_card.x1 + 14.0,
                            main_r.y0 + 106.0,
                            main_w - bar_card.width() - 14.0,
                            240.0,
                        );
                        p.fill_rect(line_card, 20.0, surface);
                        p.stroke_rect(line_card, 20.0, 1.0, border);
                        p.text(
                            "6-Month Cashflow",
                            Point::new(line_card.x0 + 20.0, line_card.y0 + 16.0),
                            &TextStyle::new(15.0)
                                .weight(FontWeight::BOLD)
                                .color(Color::WHITE),
                        );

                        let pts = [0.32, 0.50, 0.44, 0.68, 0.78, 0.92];
                        let reveal = ease_out_cubic(lt / 1.4);
                        let mut lp = Path::new();
                        let mut fp = Path::new();
                        let mut coords = Vec::new();
                        for (i, &v) in pts.iter().enumerate() {
                            let x =
                                line_card.x0 + 28.0 + (i as f64 / 5.0) * (line_card.width() - 56.0);
                            let y = line_card.y1 - 36.0 - v * 136.0 * reveal;
                            coords.push(Point::new(x, y));
                            if i == 0 {
                                lp.move_to(Point::new(x, y));
                                fp.move_to(Point::new(x, line_card.y1 - 32.0));
                                fp.line_to(Point::new(x, y));
                            } else {
                                lp.line_to(Point::new(x, y));
                                fp.line_to(Point::new(x, y));
                            }
                        }
                        if let Some(last) = coords.last() {
                            fp.line_to(Point::new(last.x, line_card.y1 - 32.0));
                            fp.close();
                        }
                        p.fill_path(
                            &fp,
                            Brush::linear(
                                Point::new(0.0, line_card.y0 + 50.0),
                                Point::new(0.0, line_card.y1 - 32.0),
                                [(0.0, mint.with_alpha(0.30)), (1.0, mint.with_alpha(0.0))],
                            ),
                        );
                        p.stroke_path(&lp, 6.0, mint.with_alpha(0.22));
                        p.stroke_path(&lp, 2.8, mint);
                        for pt in coords {
                            p.fill_circle(pt, 4.5, Color::WHITE);
                            p.fill_circle(pt, 2.5, mint);
                        }

                        // Recent activity preview strip
                        let strip = Rect::from_xywh(
                            main_x,
                            bar_card.y1 + 14.0,
                            main_w,
                            main_r.y1 - bar_card.y1 - 14.0,
                        );
                        p.fill_rect(strip, 18.0, surface);
                        p.stroke_rect(strip, 18.0, 1.0, border);
                        let rows = [
                            ("0001", "Acme Corp Payroll", "Income", "+$4,250.00", MINT),
                            (
                                "0002",
                                "Whole Foods Market",
                                "Groceries",
                                "-$142.80",
                                VIOLET,
                            ),
                            ("0003", "Apple Developer", "Tech", "-$99.00", CYAN),
                        ];
                        for (idx, (id, m, cat, amt, hex)) in rows.into_iter().enumerate() {
                            let ry = strip.y0 + 14.0 + idx as f64 * 42.0;
                            p.text(
                                id,
                                Point::new(strip.x0 + 18.0, ry + 8.0),
                                &TextStyle::new(12.5)
                                    .family(FontFamily::Monospace)
                                    .color(soft),
                            );
                            p.text(
                                m,
                                Point::new(strip.x0 + 90.0, ry + 8.0),
                                &TextStyle::new(13.5)
                                    .weight(FontWeight::SEMIBOLD)
                                    .color(Color::WHITE),
                            );
                            let cc = Color::from_hex(hex);
                            p.fill_rect(
                                Rect::from_xywh(strip.x0 + 360.0, ry + 4.0, 96.0, 24.0),
                                999.0,
                                cc.with_alpha(0.18),
                            );
                            draw_centered_text(
                                p,
                                cat,
                                strip.x0 + 408.0,
                                ry + 9.0,
                                &TextStyle::new(11.5).weight(FontWeight::BOLD).color(cc),
                            );
                            p.text(
                                amt,
                                Point::new(strip.x1 - 110.0, ry + 8.0),
                                &TextStyle::new(13.5).weight(FontWeight::BOLD).color(
                                    if amt.starts_with('+') {
                                        mint
                                    } else {
                                        Color::WHITE
                                    },
                                ),
                            );
                        }
                    });
                });
            }

            // ===== SECTION 2: TRANSACTIONS (100k-Row Virtualised Table + Search + Sort + Multi-Select) =====
            if (3.45..7.45).contains(&lt) {
                let s2_in = smoothstep(3.45, 3.85, lt) as f32;
                let s2_out = (1.0 - smoothstep(6.95, 7.35, lt)) as f32;
                let s2_a = s2_in * s2_out;
                let slide_x = (1.0 - smootherstep(3.45, 3.85, lt)) * 42.0
                    - smootherstep(6.95, 7.35, lt) * 42.0;

                p.with_opacity(s2_a, main_r, |p| {
                    p.with_transform(Transform::translate(Vec2::new(slide_x, 0.0)), |p| {
                        // Top toolbar: Search input + Category filter pills
                        let search_r = Rect::from_xywh(main_x, main_r.y0, 310.0, 40.0);
                        p.fill_rect(search_r, 12.0, surface);
                        p.stroke_rect(search_r, 12.0, 1.4, cyan.with_alpha(0.75));
                        draw_icon_at(
                            p,
                            IconName::Search,
                            Point::new(search_r.x0 + 20.0, search_r.center().y),
                            15.0,
                            cyan,
                        );
                        let typed_full = "Coffee";
                        let chars =
                            (((lt - 4.15) * 9.0).clamp(0.0, typed_full.len() as f64)) as usize;
                        let query = &typed_full[..chars];
                        let caret = if ((lt * 3.0) as i32) % 2 == 0 {
                            "|"
                        } else {
                            ""
                        };
                        p.text(
                            &format!("{query}{caret}"),
                            Point::new(search_r.x0 + 38.0, search_r.y0 + 11.0),
                            &TextStyle::new(14.0).color(Color::WHITE),
                        );

                        // Filter pills
                        let filters = [
                            ("All", false),
                            ("Dining", lt > 4.6),
                            ("Groceries", false),
                            ("Tech", false),
                        ];
                        for (i, (fl, on)) in filters.into_iter().enumerate() {
                            let fx = search_r.x1 + 14.0 + i as f64 * 96.0;
                            let fr = Rect::from_xywh(fx, main_r.y0 + 4.0, 86.0, 32.0);
                            p.fill_rect(
                                fr,
                                999.0,
                                if on { cyan.with_alpha(0.22) } else { surface },
                            );
                            p.stroke_rect(fr, 999.0, 1.1, if on { cyan } else { border });
                            draw_centered_text(
                                p,
                                fl,
                                fr.center().x,
                                fr.y0 + 8.0,
                                &TextStyle::new(12.5).weight(FontWeight::BOLD).color(if on {
                                    cyan
                                } else {
                                    soft
                                }),
                            );
                        }

                        // Virtualised Table
                        let tbl = Rect::from_xywh(
                            main_x,
                            main_r.y0 + 54.0,
                            main_w,
                            main_r.height() - 54.0,
                        );
                        p.fill_rect(tbl, 18.0, surface);
                        p.stroke_rect(tbl, 18.0, 1.0, border);
                        p.fill_rect(
                            Rect::from_xywh(tbl.x0, tbl.y0, tbl.width(), 38.0),
                            18.0,
                            Color::from_hex(0x1E1A36),
                        );

                        let hdr = TextStyle::new(12.0).weight(FontWeight::BOLD).color(soft);
                        p.text("ROW #", Point::new(tbl.x0 + 18.0, tbl.y0 + 11.0), &hdr);
                        p.text("MERCHANT", Point::new(tbl.x0 + 108.0, tbl.y0 + 11.0), &hdr);
                        p.text("CATEGORY", Point::new(tbl.x0 + 390.0, tbl.y0 + 11.0), &hdr);
                        p.text("DATE", Point::new(tbl.x0 + 550.0, tbl.y0 + 11.0), &hdr);
                        let sorted = lt > 5.35;
                        p.text(
                            if sorted { "AMOUNT ▼" } else { "AMOUNT" },
                            Point::new(tbl.x1 - 110.0, tbl.y0 + 11.0),
                            &TextStyle::new(12.0)
                                .weight(FontWeight::BOLD)
                                .color(if sorted { cyan } else { soft }),
                        );

                        let body = Rect::from_xywh(
                            tbl.x0,
                            tbl.y0 + 38.0,
                            tbl.width(),
                            tbl.height() - 40.0,
                        );
                        let scroll_px = ((lt - 3.8).max(0.0)) * 56.0;
                        let sample = [
                            ("Blue Bottle Coffee", "Dining", "Nov 27", "-$6.75", AMBER),
                            ("Sightglass Roasters", "Dining", "Nov 26", "-$18.40", AMBER),
                            ("Verve Coffee Bar", "Dining", "Nov 25", "-$9.20", AMBER),
                            ("Ritual Coffee", "Dining", "Nov 24", "-$7.50", AMBER),
                            ("Philz Mint Mojito", "Dining", "Nov 23", "-$6.95", AMBER),
                            ("Stumptown Espresso", "Dining", "Nov 22", "-$14.80", AMBER),
                            ("Tartine Bakery", "Dining", "Nov 21", "-$24.60", AMBER),
                            ("Equinox Cafe", "Health", "Nov 20", "-$12.50", MINT),
                            ("Sweetgreen Bowl", "Dining", "Nov 19", "-$17.90", AMBER),
                            ("Whole Foods", "Groceries", "Nov 18", "-$142.80", VIOLET),
                        ];
                        p.with_clip(body, 12.0, |p| {
                            let row_h = 42.0;
                            let first = (scroll_px / row_h).floor() as usize;
                            for k in 0..12 {
                                let idx = first + k;
                                let ry = body.y0 + idx as f64 * row_h - scroll_px;
                                let (m, cat, dt_s, amt, hex) = sample[idx % sample.len()];
                                let selected = lt > 5.85 && (k == 2 || k == 3);
                                if selected {
                                    p.fill_rect(
                                        Rect::from_xywh(
                                            body.x0 + 6.0,
                                            ry + 2.0,
                                            body.width() - 12.0,
                                            row_h - 4.0,
                                        ),
                                        10.0,
                                        cyan.with_alpha(0.16),
                                    );
                                    p.stroke_rect(
                                        Rect::from_xywh(
                                            body.x0 + 6.0,
                                            ry + 2.0,
                                            body.width() - 12.0,
                                            row_h - 4.0,
                                        ),
                                        10.0,
                                        1.0,
                                        cyan.with_alpha(0.55),
                                    );
                                } else if idx % 2 == 1 {
                                    p.fill_rect(
                                        Rect::from_xywh(body.x0, ry, body.width(), row_h),
                                        0.0,
                                        Color::rgba(1.0, 1.0, 1.0, 0.025),
                                    );
                                }
                                p.text(
                                    &format!("{:05}", idx + 12041),
                                    Point::new(body.x0 + 18.0, ry + 13.0),
                                    &TextStyle::new(12.5)
                                        .family(FontFamily::Monospace)
                                        .color(soft),
                                );
                                p.text(
                                    m,
                                    Point::new(body.x0 + 108.0, ry + 12.0),
                                    &TextStyle::new(13.5)
                                        .weight(FontWeight::SEMIBOLD)
                                        .color(Color::WHITE),
                                );
                                let cc = Color::from_hex(hex);
                                p.fill_rect(
                                    Rect::from_xywh(body.x0 + 388.0, ry + 9.0, 88.0, 24.0),
                                    999.0,
                                    cc.with_alpha(0.18),
                                );
                                draw_centered_text(
                                    p,
                                    cat,
                                    body.x0 + 432.0,
                                    ry + 14.0,
                                    &TextStyle::new(11.5).weight(FontWeight::BOLD).color(cc),
                                );
                                p.text(
                                    dt_s,
                                    Point::new(body.x0 + 550.0, ry + 13.0),
                                    &TextStyle::new(12.5).color(soft),
                                );
                                p.text(
                                    amt,
                                    Point::new(body.x1 - 110.0, ry + 12.0),
                                    &TextStyle::new(13.5)
                                        .weight(FontWeight::BOLD)
                                        .color(Color::WHITE),
                                );
                            }
                        });
                    });
                });
            }

            // ===== SECTION 3: BUDGETS + INTERACTIVE SLIDER + IMPORT JSON MODAL =====
            if lt >= 6.95 {
                let s3_a = smoothstep(6.95, 7.35, lt) as f32;
                let slide_x = (1.0 - smootherstep(6.95, 7.35, lt)) * 42.0;

                p.with_opacity(s3_a, main_r, |p| {
                    p.with_transform(Transform::translate(Vec2::new(slide_x, 0.0)), |p| {
                        // Header row + "+ Import JSON" button
                        p.text(
                            "Monthly Budgets",
                            Point::new(main_x + 4.0, main_r.y0 + 6.0),
                            &TextStyle::new(22.0)
                                .weight(FontWeight::BLACK)
                                .color(Color::WHITE),
                        );
                        let imp_btn =
                            Rect::from_xywh(main_r.x1 - 154.0, main_r.y0 + 2.0, 150.0, 38.0);
                        p.fill_rect(imp_btn, 999.0, violet);
                        draw_centered_text(
                            p,
                            "+ Import JSON",
                            imp_btn.center().x,
                            imp_btn.y0 + 10.5,
                            &TextStyle::new(13.5)
                                .weight(FontWeight::BOLD)
                                .color(Color::WHITE),
                        );

                        // Interactive Dining slider animates from 400 -> 650 (ratio drops from 92% magenta to 56% mint!)
                        let drag_u = smootherstep(7.55, 8.45, lt);
                        let dining_cap = lerp(400.0, 650.0, drag_u);
                        let dining_ratio = (368.0 / dining_cap).clamp(0.1, 1.0);
                        let dining_col = magenta.mix(mint, drag_u as f32);

                        let budgets = [
                            (
                                "Dining & Coffee",
                                368.0,
                                dining_cap,
                                dining_ratio,
                                dining_col,
                            ),
                            ("Groceries", 520.0, 750.0, 520.0 / 750.0, cyan),
                            ("Travel & Rides", 410.0, 600.0, 410.0 / 600.0, violet),
                            ("Health & Fitness", 210.0, 320.0, 210.0 / 320.0, mint),
                        ];

                        for (i, (name, spent, cap, ratio, col)) in budgets.into_iter().enumerate() {
                            let card = Rect::from_xywh(
                                main_x,
                                main_r.y0 + 56.0 + i as f64 * 108.0,
                                main_w,
                                94.0,
                            );
                            p.fill_rect(card, 18.0, surface);
                            p.stroke_rect(
                                card,
                                18.0,
                                1.1,
                                if i == 0 { col.with_alpha(0.55) } else { border },
                            );
                            p.text(
                                name,
                                Point::new(card.x0 + 20.0, card.y0 + 16.0),
                                &TextStyle::new(15.0)
                                    .weight(FontWeight::BOLD)
                                    .color(Color::WHITE),
                            );
                            p.text(
                                &format!("${spent:.0} / ${cap:.0}"),
                                Point::new(card.x1 - 134.0, card.y0 + 16.0),
                                &TextStyle::new(14.5).weight(FontWeight::BOLD).color(col),
                            );

                            // Glowing progress bar
                            let bar_w = card.width() - 40.0;
                            let bar_r =
                                Rect::from_xywh(card.x0 + 20.0, card.y0 + 48.0, bar_w, 12.0);
                            p.fill_rect(bar_r, 6.0, Color::rgba(1.0, 1.0, 1.0, 0.08));
                            let fill_r = Rect::from_xywh(bar_r.x0, bar_r.y0, bar_w * ratio, 12.0);
                            p.shadow(
                                fill_r,
                                6.0,
                                &Shadow::new(col.with_alpha(0.45), Vec2::ZERO, 12.0),
                            );
                            p.fill_rect(fill_r, 6.0, col);
                            if i == 0 {
                                // Interactive slider thumb on Dining row
                                let thumb_x = bar_r.x0 + bar_w * lerp(0.45, 0.80, drag_u);
                                p.fill_circle(
                                    Point::new(thumb_x, bar_r.center().y),
                                    9.5,
                                    Color::WHITE,
                                );
                            }
                            p.text(
                                &format!("{:.0}% used", ratio * 100.0),
                                Point::new(card.x0 + 20.0, card.y0 + 68.0),
                                &TextStyle::new(11.5).color(soft),
                            );
                        }
                    });
                });
            }
        });

        // ===== IMPORT JSON MODAL (pops up at lt >= 8.75s) =====
        if lt >= 8.70 {
            let mt = ease_out_back((lt - 8.70) / 0.45);
            let ma = smoothstep(8.70, 9.0, lt) as f32;
            p.fill_rect(win, 24.0, Color::rgba(0.02, 0.01, 0.05, 0.62 * ma));
            let modal =
                Rect::from_xywh(win.center().x - 230.0, win.center().y - 130.0, 460.0, 260.0);
            p.with_transform(
                Transform::scale_about(0.86 + 0.14 * mt, modal.center()),
                |p| {
                    p.shadow(
                        modal,
                        24.0,
                        &Shadow::new(violet.with_alpha(0.45 * ma), Vec2::new(0.0, 16.0), 42.0),
                    );
                    p.fill_rect(modal, 24.0, Color::from_hex(0x18152C).with_alpha(ma));
                    p.stroke_rect(modal, 24.0, 1.4, violet.with_alpha(0.75 * ma));
                    p.text(
                        "Import Transactions (JSON)",
                        Point::new(modal.x0 + 26.0, modal.y0 + 24.0),
                        &TextStyle::new(18.0)
                            .weight(FontWeight::BLACK)
                            .color(Color::WHITE.with_alpha(ma)),
                    );
                    let box_r = Rect::from_xywh(
                        modal.x0 + 26.0,
                        modal.y0 + 62.0,
                        modal.width() - 52.0,
                        114.0,
                    );
                    p.fill_rect(box_r, 14.0, Color::from_hex(0x0E0C1C).with_alpha(ma));
                    p.stroke_rect(box_r, 14.0, 1.0, cyan.with_alpha(0.35 * ma));
                    let mono = TextStyle::new(12.5)
                        .family(FontFamily::Monospace)
                        .color(mint.with_alpha(ma));
                    p.text("[", Point::new(box_r.x0 + 14.0, box_r.y0 + 12.0), &mono);
                    p.text(
                        "  { \"merchant\": \"Blue Bottle\", \"amount\": -6.75 },",
                        Point::new(box_r.x0 + 14.0, box_r.y0 + 34.0),
                        &mono,
                    );
                    p.text(
                        "  { \"merchant\": \"Stripe Payout\", \"amount\": 4250.0 }",
                        Point::new(box_r.x0 + 14.0, box_r.y0 + 56.0),
                        &mono,
                    );
                    p.text("]", Point::new(box_r.x0 + 14.0, box_r.y0 + 78.0), &mono);

                    draw_pill(
                        p,
                        "Cancel",
                        modal.x1 - 186.0,
                        modal.y1 - 54.0,
                        surface.with_alpha(ma),
                        soft.with_alpha(ma),
                        border,
                    );
                    draw_pill(
                        p,
                        "Import 2 Rows",
                        modal.x1 - 88.0,
                        modal.y1 - 54.0,
                        mint.with_alpha(ma),
                        Color::from_hex(0x090814).with_alpha(ma),
                        Color::TRANSPARENT,
                    );
                },
            );
        }

        // ---- Choreographed Cursor across all 3 Minty sections ----
        let p_bar0 = Point::new(main_x + 56.0, main_r.y0 + 240.0);
        let p_bar4 = Point::new(main_x + 412.0, main_r.y0 + 210.0);
        let p_tab_tx = Point::new(sb.x0 + 96.0, sb.y0 + 128.0);
        let p_sort = Point::new(main_r.x1 - 74.0, main_r.y0 + 72.0);
        let p_row = Point::new(main_x + 260.0, main_r.y0 + 195.0);
        let p_tab_bud = Point::new(sb.x0 + 96.0, sb.y0 + 172.0);
        let p_slider0 = Point::new(main_x + 20.0 + (main_w - 40.0) * 0.45, main_r.y0 + 110.0);
        let p_slider1 = Point::new(main_x + 20.0 + (main_w - 40.0) * 0.80, main_r.y0 + 110.0);
        let p_import = Point::new(main_r.x1 - 78.0, main_r.y0 + 21.0);

        let cur = if lt < 2.0 {
            let u = smootherstep(0.3, 0.85, lt);
            Point::new(
                lerp(p_bar0.x - 60.0, p_bar0.x, u),
                lerp(p_bar0.y + 50.0, p_bar0.y, u),
            )
        } else if lt < 3.1 {
            let u = smootherstep(2.0, 2.45, lt);
            Point::new(lerp(p_bar0.x, p_bar4.x, u), lerp(p_bar0.y, p_bar4.y, u))
        } else if lt < 4.8 {
            let u = smootherstep(3.1, 3.55, lt);
            Point::new(lerp(p_bar4.x, p_tab_tx.x, u), lerp(p_bar4.y, p_tab_tx.y, u))
        } else if lt < 5.5 {
            let u = smootherstep(4.8, 5.3, lt);
            Point::new(lerp(p_tab_tx.x, p_sort.x, u), lerp(p_tab_tx.y, p_sort.y, u))
        } else if lt < 6.5 {
            let u = smootherstep(5.5, 5.82, lt);
            Point::new(lerp(p_sort.x, p_row.x, u), lerp(p_sort.y, p_row.y, u))
        } else if lt < 7.35 {
            let u = smootherstep(6.5, 6.95, lt);
            Point::new(lerp(p_row.x, p_tab_bud.x, u), lerp(p_row.y, p_tab_bud.y, u))
        } else if lt < 7.55 {
            let u = smootherstep(7.35, 7.55, lt);
            Point::new(
                lerp(p_tab_bud.x, p_slider0.x, u),
                lerp(p_tab_bud.y, p_slider0.y, u),
            )
        } else if lt < 8.45 {
            let u = smootherstep(7.55, 8.45, lt);
            Point::new(
                lerp(p_slider0.x, p_slider1.x, u),
                lerp(p_slider0.y, p_slider1.y, u),
            )
        } else {
            let u = smootherstep(8.45, 8.70, lt);
            Point::new(
                lerp(p_slider1.x, p_import.x, u),
                lerp(p_slider1.y, p_import.y, u),
            )
        };

        let clicks = [3.55, 5.35, 5.85, 7.0, 8.70];
        let click_age = clicks
            .iter()
            .filter(|&&ct| lt >= ct && lt - ct < 0.48)
            .map(|&ct| lt - ct)
            .next_back();
        draw_cursor(p, cur, click_age, cyan, fade);
    });

    let headline = if lt < 3.6 {
        "Overview · Reactive Canvas charts & hover hit-testing"
    } else if lt < 7.1 {
        "Transactions · 100k-row virtualised Table, search & sort"
    } else {
        "Budgets & Modals · Live sliders & local JSON import"
    };
    draw_editorial_hud(
        p,
        "02 / FLAGSHIP · MINTY FINANCE",
        headline,
        violet,
        fade,
        0.0,
    );
}

// ---- ACT IV: FLAGSHIP #2 — PULSE STUDIO 124 BPM BEAT-SYNCED SHOWPIECE (21.5s .. 32.0s) ---
//   0.0..3.7s : Macro 1.48x on 24-band FFT Spectrum & Glowing Oscilloscope + Hover Band Readout
//   3.7..7.1s : Smooth camera pan to Step Sequencer + Synth Rack; cursor arms pads & switches Sine -> Saw
//   7.1..10.5s: Wide 1.0x pullback revealing the entire live Pulse Studio workstation + waveform Table

fn draw_act_studio(p: &mut Painter<'_>, lt: f64, global_t: f64) {
    let fade = (smoothstep(0.0, 0.4, lt) * (1.0 - smoothstep(9.9, 10.5, lt))) as f32;
    let (beat_idx, _, pulse) = music_clock(global_t);
    let cur_step = beat_idx % 8;

    let win = Rect::from_xywh(100.0, 44.0, 1080.0, 566.0);
    let surface = Color::from_hex(0x161328);
    let border = Color::rgba(0.95, 0.94, 1.0, 0.10);
    let soft = Color::from_hex(0x9E98BF);
    let cyan = Color::from_hex(CYAN);
    let violet = Color::from_hex(VIOLET);
    let magenta = Color::from_hex(MAGENTA);
    let mint = Color::from_hex(MINT);

    // Cinema camera sweep: Macro FFT -> Sequencer + Rack -> Wide full workstation
    let (cam_x, cam_y, zoom) = if lt < 3.7 {
        let u = smootherstep(0.0, 3.7, lt);
        (
            lerp(365.0, 420.0, u),
            lerp(230.0, 240.0, u),
            lerp(1.50, 1.38, u),
        )
    } else if lt < 7.1 {
        let u = smootherstep(3.7, 5.2, lt);
        (
            lerp(420.0, 860.0, u),
            lerp(240.0, 340.0, u),
            lerp(1.38, 1.24, u),
        )
    } else {
        let u = smootherstep(7.1, 8.9, lt);
        (
            lerp(860.0, 640.0, u),
            lerp(340.0, 330.0, u),
            lerp(1.24, 1.01, u),
        )
    };

    // Live interactive state driven by cursor:
    let extra_pad1 = lt >= 4.75;
    let extra_pad2 = lt >= 5.25;
    let saw_mode = lt >= 6.25;
    let cutoff = lerp(0.54, 0.88, smootherstep(5.55, 6.15, lt));

    with_camera(p, Point::new(cam_x, cam_y), zoom, |p| {
        draw_neon_window(
            p,
            win,
            "Pulse Studio · 124 BPM Creative Workstation",
            cyan,
            0.0,
        );

        // ---- 1. Live 24-Band FFT Spectrum & Oscilloscope ----
        let spec_card = Rect::from_xywh(win.x0 + 18.0, win.y0 + 44.0, 540.0, 272.0);
        p.fill_rect(spec_card, 20.0, surface);
        p.stroke_rect(spec_card, 20.0, 1.0, border);
        p.text(
            "Live Spectrum & Oscilloscope  ·  124 BPM",
            Point::new(spec_card.x0 + 18.0, spec_card.y0 + 14.0),
            &TextStyle::new(14.0)
                .weight(FontWeight::BOLD)
                .color(Color::WHITE),
        );

        let hovered_band = if lt < 3.4 {
            Some(((lt * 4.5) as usize % 14) + 4)
        } else {
            None
        };
        let readout = if let Some(hb) = hovered_band {
            format!(
                "Band {:02} · {} Hz · -{:.1} dB",
                hb + 1,
                60 + hb * 140,
                1.4 + (hb % 5) as f64 * 0.7
            )
        } else if saw_mode {
            "Wave: SAWTOOTH · Cutoff 88%".to_string()
        } else {
            "24-band FFT · phase-locked".to_string()
        };
        p.text(
            &readout,
            Point::new(spec_card.x1 - 210.0, spec_card.y0 + 15.0),
            &TextStyle::new(12.0).weight(FontWeight::BOLD).color(cyan),
        );

        let screen = Rect::from_xywh(
            spec_card.x0 + 14.0,
            spec_card.y0 + 42.0,
            spec_card.width() - 28.0,
            spec_card.height() - 56.0,
        );
        p.fill_rect(screen, 14.0, Color::from_hex(0x070611));
        p.stroke_rect(screen, 14.0, 1.0, cyan.with_alpha(0.18));

        // Subtle horizontal dB grid lines
        for g in 1..4 {
            let gy = screen.y0 + (g as f64 / 4.0) * screen.height();
            p.stroke_line(
                Point::new(screen.x0 + 8.0, gy),
                Point::new(screen.x1 - 8.0, gy),
                0.8,
                Color::rgba(1.0, 1.0, 1.0, 0.05),
            );
        }

        let bands = 24usize;
        let sw = (screen.width() - 24.0) / bands as f64;
        for b in 0..bands {
            let bf = b as f64 / bands as f64;
            let bass_boost = if b < 8 { 0.45 * pulse } else { 0.18 * pulse };
            let harmonic = (global_t * (4.2 + bf * 7.5) + b as f64 * 0.52).sin().abs();
            let amp =
                (0.16 + 0.56 * harmonic * (0.65 + 0.45 * cutoff) + bass_boost).clamp(0.08, 0.94);
            let bh = (screen.height() - 24.0) * amp;
            let bx = screen.x0 + 12.0 + b as f64 * sw + sw * 0.16;
            let by = screen.y1 - 12.0 - bh;
            let top = violet
                .mix(cyan, (bf * 1.4).min(1.0) as f32)
                .mix(mint, ((bf - 0.55).max(0.0) * 2.2) as f32);
            let bar_r = Rect::from_xywh(bx, by, sw * 0.68, bh);
            if hovered_band == Some(b) {
                p.shadow(
                    bar_r,
                    5.0,
                    &Shadow::new(cyan.with_alpha(0.75), Vec2::ZERO, 14.0),
                );
            }
            p.fill_rect(
                bar_r,
                4.5,
                Brush::linear(
                    Point::new(bx, by),
                    Point::new(bx, by + bh),
                    [(0.0, top), (1.0, violet.with_alpha(0.22))],
                ),
            );
            p.fill_circle(
                Point::new(bx + sw * 0.34, by - 5.0),
                2.2,
                if hovered_band == Some(b) {
                    Color::WHITE
                } else {
                    top
                },
            );
        }

        // Morphing Neon Oscilloscope (Sine -> Sawtooth when cursor switches wave shape!)
        let saw_mix = smootherstep(6.25, 6.65, lt);
        let mut osc = Path::new();
        for s in 0..84 {
            let u = s as f64 / 83.0;
            let x = screen.x0 + 12.0 + u * (screen.width() - 24.0);
            let phase = u * TAU * 3.0 + global_t * 7.2;
            let sine_v = phase.sin();
            let saw_v = (2.0 / PI)
                * ((phase * 0.5).sin() - (phase).sin() * 0.5 + (phase * 1.5).sin() * 0.33);
            let y = screen.center().y
                + lerp(sine_v, saw_v * 1.25, saw_mix) * (38.0 + 18.0 * pulse) * (u * PI).sin();
            if s == 0 {
                osc.move_to(Point::new(x, y));
            } else {
                osc.line_to(Point::new(x, y));
            }
        }
        let osc_col = cyan.mix(magenta, saw_mix as f32);
        p.stroke_path(&osc, 8.0, osc_col.with_alpha(0.32));
        p.stroke_path(&osc, 2.4, Color::WHITE);

        // ---- 2. 4×8 Step Sequencer (Beat-Synced!) ----
        let seq_card = Rect::from_xywh(spec_card.x1 + 16.0, win.y0 + 44.0, 488.0, 272.0);
        p.fill_rect(seq_card, 20.0, surface);
        p.stroke_rect(seq_card, 20.0, 1.0, border);
        p.text(
            "Step Sequencer (8-Step)",
            Point::new(seq_card.x0 + 18.0, seq_card.y0 + 14.0),
            &TextStyle::new(14.0)
                .weight(FontWeight::BOLD)
                .color(Color::WHITE),
        );
        p.text(
            &format!("Step {} / 8", cur_step + 1),
            Point::new(seq_card.x1 - 86.0, seq_card.y0 + 14.0),
            &TextStyle::new(12.5).weight(FontWeight::BOLD).color(mint),
        );

        let pat = [
            [true, false, false, false, true, false, true, false],
            [false, false, true, false, false, false, true, false],
            [true, true, true, true, true, true, true, true],
            [true, extra_pad1, false, true, extra_pad2, true, false, true],
        ];
        let tcols = [MAGENTA, AMBER, MINT, CYAN];
        let tnames = ["Kick", "Snare", "Hi-Hat", "Synth"];
        for (tr, row_pat) in pat.iter().enumerate() {
            let ry = seq_card.y0 + 54.0 + tr as f64 * 50.0;
            let tc = Color::from_hex(tcols[tr]);
            p.text(
                tnames[tr],
                Point::new(seq_card.x0 + 18.0, ry + 10.0),
                &TextStyle::new(13.0).weight(FontWeight::BOLD).color(tc),
            );
            for (st, &on) in row_pat.iter().enumerate() {
                let px = seq_card.x0 + 78.0 + st as f64 * 49.0;
                let active = st == cur_step;
                let base = Rect::from_xywh(px, ry, 40.0, 38.0);
                let pad = if on && active {
                    base.inset(-2.5 * pulse)
                } else {
                    base
                };
                let bg = if on {
                    if active {
                        tc.mix(Color::WHITE, 0.35)
                    } else {
                        tc
                    }
                } else if active {
                    Color::rgba(1.0, 1.0, 1.0, 0.16)
                } else {
                    Color::rgba(1.0, 1.0, 1.0, 0.06)
                };
                if on && active {
                    p.shadow(
                        pad,
                        10.0,
                        &Shadow::new(tc.with_alpha(0.75), Vec2::ZERO, 16.0),
                    );
                }
                p.fill_rect(pad, 10.0, bg);
                p.stroke_rect(
                    pad,
                    10.0,
                    if active { 1.5 } else { 1.0 },
                    if active {
                        Color::WHITE.with_alpha(0.8)
                    } else {
                        border
                    },
                );
            }
        }

        // ---- 3. Synth & FX Rack (Sliders + Wave Radio) ----
        let fx_card = Rect::from_xywh(win.x0 + 18.0, spec_card.y1 + 16.0, 480.0, 218.0);
        p.fill_rect(fx_card, 20.0, surface);
        p.stroke_rect(fx_card, 20.0, 1.0, border);
        p.text(
            "Synth & FX Rack",
            Point::new(fx_card.x0 + 18.0, fx_card.y0 + 14.0),
            &TextStyle::new(14.0)
                .weight(FontWeight::BOLD)
                .color(Color::WHITE),
        );

        let sliders = [
            ("Cutoff", cutoff, cyan),
            ("Resonance", 0.44 + 0.18 * (global_t * 1.1).cos(), violet),
            ("Reverb", 0.36 + 0.14 * (global_t * 0.8).sin(), magenta),
        ];
        for (i, (lbl, val, col)) in sliders.into_iter().enumerate() {
            let sy = fx_card.y0 + 50.0 + i as f64 * 38.0;
            p.text(
                lbl,
                Point::new(fx_card.x0 + 18.0, sy),
                &TextStyle::new(13.0).color(soft),
            );
            let tr = Rect::from_xywh(fx_card.x0 + 112.0, sy + 5.0, 286.0, 7.0);
            p.fill_rect(tr, 4.0, Color::rgba(1.0, 1.0, 1.0, 0.10));
            p.fill_rect(
                Rect::from_xywh(tr.x0, tr.y0, tr.width() * val, 7.0),
                4.0,
                col,
            );
            let thumb = Point::new(tr.x0 + tr.width() * val, tr.center().y);
            p.shadow(
                Rect::from_xywh(thumb.x - 7.0, thumb.y - 7.0, 14.0, 14.0),
                7.0,
                &Shadow::new(col.with_alpha(0.65), Vec2::ZERO, 10.0),
            );
            p.fill_circle(thumb, 8.5, Color::WHITE);
            p.text(
                &format!("{:.0}%", val * 100.0),
                Point::new(tr.x1 + 14.0, sy),
                &TextStyle::new(12.5).weight(FontWeight::BOLD).color(col),
            );
        }

        // Wave-shape Radio buttons (Sine -> Saw live switch!)
        let waves = [("Sine", !saw_mode), ("Saw", saw_mode), ("Square", false)];
        for (i, (wlbl, selected)) in waves.into_iter().enumerate() {
            let wx = fx_card.x0 + 24.0 + i as f64 * 110.0;
            let wy = fx_card.y1 - 36.0;
            p.stroke_rect(
                Rect::from_xywh(wx, wy, 18.0, 18.0),
                999.0,
                1.8,
                if selected {
                    magenta
                } else {
                    soft.with_alpha(0.5)
                },
            );
            if selected {
                p.fill_circle(Point::new(wx + 9.0, wy + 9.0), 5.0, magenta);
            }
            p.text(
                wlbl,
                Point::new(wx + 26.0, wy + 1.0),
                &TextStyle::new(13.0)
                    .weight(FontWeight::BOLD)
                    .color(if selected { Color::WHITE } else { soft }),
            );
        }

        // ---- 4. Track Stems Table with Custom Animated Mini-Waveform Cells ----
        let tbl_card = Rect::from_xywh(fx_card.x1 + 16.0, spec_card.y1 + 16.0, 548.0, 218.0);
        p.fill_rect(tbl_card, 20.0, surface);
        p.stroke_rect(tbl_card, 20.0, 1.0, border);
        p.text(
            "Track Stems  ·  Custom Waveform Cells",
            Point::new(tbl_card.x0 + 18.0, tbl_card.y0 + 14.0),
            &TextStyle::new(14.0)
                .weight(FontWeight::BOLD)
                .color(Color::WHITE),
        );
        let stems = [
            ("Neon Kick 808", "-1.2 dB", MAGENTA, 1.1),
            ("Crisp Clap & Rim", "-3.5 dB", AMBER, 2.3),
            ("Velvet Pad Chords", "-4.8 dB", VIOLET, 3.7),
            ("Glass Arp Lead", "-2.4 dB", MINT, 5.1),
        ];
        for (i, (name, gain, hex, seed)) in stems.into_iter().enumerate() {
            let ry = tbl_card.y0 + 50.0 + i as f64 * 39.0;
            let col = Color::from_hex(hex);
            p.text(
                name,
                Point::new(tbl_card.x0 + 18.0, ry + 6.0),
                &TextStyle::new(13.0)
                    .weight(FontWeight::SEMIBOLD)
                    .color(Color::WHITE),
            );
            let wx0 = tbl_card.x0 + 190.0;
            for b in 0..24 {
                let h = 5.0
                    + 19.0
                        * ((b as f64 * 0.72 + seed + global_t * 3.0).sin().abs())
                        * (0.7 + 0.3 * pulse);
                p.fill_rect(
                    Rect::from_xywh(wx0 + b as f64 * 10.5, ry + 14.0 - h * 0.5, 4.5, h),
                    2.5,
                    col,
                );
            }
            p.text(
                gain,
                Point::new(tbl_card.x1 - 74.0, ry + 6.0),
                &TextStyle::new(12.0)
                    .family(FontFamily::Monospace)
                    .color(soft),
            );
        }

        // ---- Choreographed Cursor across Pulse Studio ----
        let p_fft0 = Point::new(screen.x0 + 95.0, screen.center().y + 10.0);
        let p_fft1 = Point::new(screen.x1 - 95.0, screen.center().y - 12.0);
        let p_pad1 = Point::new(
            seq_card.x0 + 78.0 + 49.0 + 20.0,
            seq_card.y0 + 54.0 + 150.0 + 19.0,
        );
        let p_pad2 = Point::new(
            seq_card.x0 + 78.0 + 4.0 * 49.0 + 20.0,
            seq_card.y0 + 54.0 + 150.0 + 19.0,
        );
        let p_cut0 = Point::new(fx_card.x0 + 112.0 + 286.0 * 0.54, fx_card.y0 + 55.0);
        let p_cut1 = Point::new(fx_card.x0 + 112.0 + 286.0 * 0.88, fx_card.y0 + 55.0);
        let p_saw = Point::new(fx_card.x0 + 143.0, fx_card.y1 - 27.0);

        let cur = if lt < 3.4 {
            let u = smootherstep(0.3, 3.2, lt);
            Point::new(lerp(p_fft0.x, p_fft1.x, u), lerp(p_fft0.y, p_fft1.y, u))
        } else if lt < 4.8 {
            let u = smootherstep(3.8, 4.7, lt);
            Point::new(lerp(p_fft1.x, p_pad1.x, u), lerp(p_fft1.y, p_pad1.y, u))
        } else if lt < 5.3 {
            let u = smootherstep(4.85, 5.22, lt);
            Point::new(lerp(p_pad1.x, p_pad2.x, u), lerp(p_pad1.y, p_pad2.y, u))
        } else if lt < 5.55 {
            let u = smootherstep(5.3, 5.55, lt);
            Point::new(lerp(p_pad2.x, p_cut0.x, u), lerp(p_pad2.y, p_cut0.y, u))
        } else if lt < 6.15 {
            let u = smootherstep(5.55, 6.15, lt);
            Point::new(lerp(p_cut0.x, p_cut1.x, u), lerp(p_cut0.y, p_cut1.y, u))
        } else {
            let u = smootherstep(6.15, 6.27, lt);
            Point::new(lerp(p_cut1.x, p_saw.x, u), lerp(p_cut1.y, p_saw.y, u))
        };

        let clicks = [4.75, 5.25, 6.25];
        let click_age = clicks
            .iter()
            .filter(|&&ct| lt >= ct && lt - ct < 0.48)
            .map(|&ct| lt - ct)
            .next_back();
        draw_cursor(p, cur, click_age, cyan, fade);
    });

    let headline = if lt < 3.7 {
        "24-band FFT & oscilloscope · phase-locked at 124 BPM"
    } else if lt < 7.1 {
        "Interactive sequencer & live Sine → Sawtooth wave morph"
    } else {
        "Full workstation · custom waveform cells & zero lag"
    };
    draw_editorial_hud(p, "03 / FLAGSHIP · PULSE STUDIO", headline, cyan, fade, 0.0);
}

// ---- ACT V: THEME MORPH, SUBTREE, RTL, CJK IME & ⌘⌥I INSPECTOR (32.0s .. 37.5s) ---------

fn draw_act_system(p: &mut Painter<'_>, lt: f64) -> f32 {
    let fade = (smoothstep(0.0, 0.4, lt) * (1.0 - smoothstep(4.9, 5.5, lt))) as f32;
    let light_mix = (smootherstep(0.9, 1.8, lt) * (1.0 - smootherstep(4.1, 4.9, lt))) as f32;
    let show_inspector = lt >= 2.35;

    let surface = Color::from_hex(0x161328).mix(Color::WHITE, light_mix);
    let ink = Color::from_hex(0xF6F5FF).mix(Color::from_hex(0x16142A), light_mix);
    let soft = Color::from_hex(0x9E98BF).mix(Color::from_hex(0x6B6687), light_mix);
    let accent = Color::from_hex(CYAN).mix(Color::from_hex(VIOLET), light_mix);
    let coral = Color::from_hex(MAGENTA);

    let zoom = lerp(1.06, 1.0, smootherstep(0.0, 1.2, lt));
    with_camera(p, Point::new(LOGICAL_W * 0.5, 326.0), zoom, |p| {
        let win = Rect::from_xywh(120.0, 48.0, 1040.0, 540.0);
        draw_neon_window(
            p,
            win,
            "rugkit · Widget Gallery, RTL, IME & ⌘⌥I Inspector",
            accent,
            light_mix,
        );

        // Theme toggle pill in top-right of titlebar
        let theme_btn = Rect::from_xywh(win.x1 - 122.0, win.y0 + 10.0, 102.0, 26.0);
        p.fill_rect(theme_btn, 999.0, accent.with_alpha(0.20));
        p.stroke_rect(theme_btn, 999.0, 1.0, accent);
        p.fill_circle(
            Point::new(theme_btn.x0 + 16.0, theme_btn.center().y),
            4.5,
            if light_mix > 0.5 {
                Color::from_hex(AMBER)
            } else {
                accent
            },
        );
        p.text(
            if light_mix > 0.5 {
                "Daylight"
            } else {
                "Neon Dark"
            },
            Point::new(theme_btn.x0 + 28.0, theme_btn.y0 + 6.0),
            &TextStyle::new(11.5).weight(FontWeight::BOLD).color(ink),
        );

        let cards = [
            Rect::from_xywh(win.x0 + 22.0, win.y0 + 50.0, 486.0, 218.0),
            Rect::from_xywh(win.x0 + 530.0, win.y0 + 50.0, 486.0, 218.0),
            Rect::from_xywh(win.x0 + 22.0, win.y0 + 288.0, 486.0, 218.0),
            Rect::from_xywh(win.x0 + 530.0, win.y0 + 288.0, 486.0, 218.0),
        ];
        let titles = [
            "Buttons, Pills & Bundled Lucide Icons",
            "Animated Springs · Toggles & Sliders",
            "Subtree Theme Override & RTL Mirroring",
            "CJK IME TextInput & AccessKit VoiceOver",
        ];

        for (i, (&cr, &title)) in cards.iter().zip(titles.iter()).enumerate() {
            p.fill_rect(cr, 20.0, surface);
            p.stroke_rect(cr, 20.0, 1.0, ink.with_alpha(0.12));
            p.text(
                title,
                Point::new(cr.x0 + 20.0, cr.y0 + 18.0),
                &TextStyle::new(14.5).weight(FontWeight::BOLD).color(ink),
            );
            match i {
                0 => {
                    draw_pill(
                        p,
                        "Primary",
                        cr.x0 + 74.0,
                        cr.y0 + 64.0,
                        accent,
                        Color::from_hex(0x090814).mix(Color::WHITE, light_mix),
                        Color::TRANSPARENT,
                    );
                    draw_pill(
                        p,
                        "Secondary",
                        cr.x0 + 194.0,
                        cr.y0 + 64.0,
                        surface,
                        ink,
                        ink.with_alpha(0.18),
                    );
                    draw_pill(
                        p,
                        "Neon Coral",
                        cr.x0 + 322.0,
                        cr.y0 + 64.0,
                        coral,
                        Color::WHITE,
                        Color::TRANSPARENT,
                    );
                    for (k, ic) in [
                        IconName::Heart,
                        IconName::Star,
                        IconName::Wallet,
                        IconName::TrendingUp,
                        IconName::Sliders,
                        IconName::Bell,
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        draw_icon_at(
                            p,
                            ic,
                            Point::new(cr.x0 + 42.0 + k as f64 * 56.0, cr.y0 + 150.0),
                            26.0,
                            accent,
                        );
                    }
                }
                1 => {
                    let tog = smootherstep(0.6, 1.5, lt);
                    p.fill_rect(
                        Rect::from_xywh(cr.x0 + 24.0, cr.y0 + 66.0, 56.0, 30.0),
                        999.0,
                        accent,
                    );
                    p.fill_circle(
                        Point::new(cr.x0 + 39.0 + tog * 26.0, cr.y0 + 81.0),
                        11.5,
                        Color::WHITE,
                    );
                    p.text(
                        "Interruptible Spring Physics",
                        Point::new(cr.x0 + 96.0, cr.y0 + 72.0),
                        &TextStyle::new(14.5).color(ink),
                    );

                    let sl = 0.22 + 0.68 * smootherstep(0.4, 2.2, lt);
                    p.fill_rect(
                        Rect::from_xywh(cr.x0 + 24.0, cr.y0 + 144.0, 410.0, 8.0),
                        4.0,
                        ink.with_alpha(0.14),
                    );
                    p.fill_rect(
                        Rect::from_xywh(cr.x0 + 24.0, cr.y0 + 144.0, 410.0 * sl, 8.0),
                        4.0,
                        accent,
                    );
                    p.fill_circle(
                        Point::new(cr.x0 + 24.0 + 410.0 * sl, cr.y0 + 148.0),
                        10.5,
                        Color::WHITE,
                    );
                }
                2 => {
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
                        cr.x0 + 68.0,
                        cr.y0 + 134.0,
                        coral,
                        Color::WHITE,
                        Color::TRANSPARENT,
                    );
                    draw_pill(
                        p,
                        "First",
                        cr.x0 + 162.0,
                        cr.y0 + 134.0,
                        surface,
                        ink,
                        ink.with_alpha(0.18),
                    );
                    p.text(
                        "RTL mirrored automatically →",
                        Point::new(cr.x0 + 220.0, cr.y0 + 142.0),
                        &TextStyle::new(13.5).color(soft),
                    );
                }
                _ => {
                    let inp = Rect::from_xywh(cr.x0 + 22.0, cr.y0 + 64.0, 436.0, 44.0);
                    p.fill_rect(inp, 12.0, surface);
                    p.stroke_rect(inp, 12.0, 1.8, accent);
                    let full = "こんにちは世界 · CJK IME Ready!";
                    let count = ((lt * 10.0) as usize).min(full.chars().count());
                    let shown: String = full.chars().take(count).collect();
                    p.text(
                        &shown,
                        Point::new(inp.x0 + 16.0, inp.y0 + 12.0),
                        &TextStyle::new(15.0).color(ink),
                    );
                    p.text(
                        "macOS VoiceOver AccessKit + Headless Harness",
                        Point::new(cr.x0 + 24.0, cr.y0 + 138.0),
                        &TextStyle::new(13.5).color(soft),
                    );
                }
            }
            if show_inspector {
                p.stroke_rect(cr, 4.0, 1.5, Color::from_hex(CYAN).with_alpha(0.9));
            }
        }

        // Floating ⌘⌥I Inspector HUD
        if show_inspector {
            let ins_t = ease_out_back((lt - 2.35) / 0.45);
            let hud = Rect::from_xywh(
                win.x1 - 244.0,
                win.y1 - 154.0 + (1.0 - ins_t) * 28.0,
                276.0,
                148.0,
            );
            p.shadow(
                hud,
                18.0,
                &Shadow::new(Color::rgba(0.0, 0.0, 0.0, 0.48), Vec2::new(0.0, 12.0), 26.0),
            );
            p.fill_rect(hud, 18.0, Color::rgba(0.06, 0.05, 0.12, 0.96));
            p.stroke_rect(hud, 18.0, 1.5, Color::from_hex(CYAN));
            p.text(
                "rugkit inspector  (⌘⌥I)",
                Point::new(hud.x0 + 14.0, hud.y0 + 12.0),
                &TextStyle::new(13.5)
                    .weight(FontWeight::BOLD)
                    .color(Color::from_hex(CYAN)),
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
                &TextStyle::new(11.0).color(Color::from_hex(0xA6A1C4)),
            );
        }

        // Cursor clicking the Daylight theme toggle
        let cur = Point::new(
            lerp(
                win.center().x,
                theme_btn.center().x,
                smootherstep(0.15, 0.85, lt),
            ),
            lerp(
                win.center().y,
                theme_btn.center().y,
                smootherstep(0.15, 0.85, lt),
            ),
        );
        let click_age = (0.90..1.38).contains(&lt).then_some(lt - 0.90);
        draw_cursor(p, cur, click_age, accent, fade);
    });

    draw_editorial_hud(
        p,
        "04 / DESIGN SYSTEM & DEV TOOLS",
        "Animated Light & Dark · Subtree Themes · RTL · ⌘⌥I Inspector",
        accent,
        fade,
        light_mix,
    );
    light_mix
}

// ---- ACT VI: HARD NUMBERS & NEON HORIZON OUTRO (37.5s .. 42.0s) -------------------------

fn draw_act_finale(p: &mut Painter<'_>, lt: f64, global_t: f64) {
    let (_, _, pulse) = music_clock(global_t);

    // 0.0..2.2s: Clean editorial performance stats
    if lt < 2.35 {
        let a = (smoothstep(0.0, 0.35, lt) * (1.0 - smoothstep(1.9, 2.35, lt))) as f32;
        draw_centered_text(
            p,
            "MEASURED ON APPLE SILICON · BUILT FOR 120FPS",
            LOGICAL_W * 0.5,
            106.0,
            &TextStyle::new(13.0)
                .weight(FontWeight::BLACK)
                .letter_spacing(3.0)
                .color(Color::from_hex(CYAN).with_alpha(a)),
        );

        let stats = [
            (
                "< 8 ms",
                "Flagship Frame Time",
                "1.9ms CPU · 7.7ms GPU",
                CYAN,
            ),
            ("139 ms", "Cold Start", "Launch to first frame", MINT),
            ("7.9 MB", "Release Binary", "Stripped hello-world", MAGENTA),
            (
                "100k+",
                "Virtualised Rows",
                "+ cargo rugkit dev hot-reload",
                VIOLET,
            ),
        ];
        for (i, (big, label, sub, hex)) in stats.into_iter().enumerate() {
            let enter = ease_out_back((lt - i as f64 * 0.08) / 0.45);
            let cx = 195.0 + i as f64 * 296.0;
            let cy = 315.0 + (1.0 - enter) * 28.0;
            let r = Rect::from_xywh(cx - 134.0, cy - 108.0, 268.0, 216.0);
            let col = Color::from_hex(hex);
            p.shadow(
                r,
                24.0,
                &Shadow::new(col.with_alpha(0.22 * a), Vec2::new(0.0, 10.0), 28.0),
            );
            p.fill_rect(r, 24.0, Color::rgba(0.08, 0.07, 0.16, 0.92 * a));
            p.stroke_rect(r, 24.0, 1.3, col.with_alpha(0.55 * a));
            draw_centered_text(
                p,
                big,
                cx,
                r.y0 + 38.0,
                &TextStyle::new(50.0)
                    .weight(FontWeight::BLACK)
                    .letter_spacing(-2.0)
                    .color(col.with_alpha(a)),
            );
            draw_centered_text(
                p,
                label,
                cx,
                r.y0 + 114.0,
                &TextStyle::new(16.0)
                    .weight(FontWeight::BOLD)
                    .color(Color::WHITE.with_alpha(a)),
            );
            draw_centered_text(
                p,
                sub,
                cx,
                r.y0 + 146.0,
                &TextStyle::new(12.5).color(Color::from_hex(0xA6A1C4).with_alpha(a)),
            );
        }
    }

    // 2.1..4.5s: Serene Neon Horizon Outro
    if lt >= 2.05 {
        let ot = lt - 2.05;
        let a = (smoothstep(0.0, 0.45, ot) * (1.0 - smoothstep(2.1, 2.45, ot))) as f32;
        let sc = 0.88 + 0.12 * ease_out_back(ot / 0.65);

        p.with_transform(
            Transform::scale_about(sc, Point::new(LOGICAL_W * 0.5, 290.0)),
            |p| {
                p.shadow(
                    Rect::from_xywh(LOGICAL_W * 0.5 - 230.0, 214.0, 460.0, 96.0),
                    48.0,
                    &Shadow::new(
                        Color::from_hex(CYAN).with_alpha((0.34 + 0.12 * pulse as f32) * a),
                        Vec2::new(18.0, 0.0),
                        54.0,
                    ),
                );
                p.shadow(
                    Rect::from_xywh(LOGICAL_W * 0.5 - 230.0, 214.0, 460.0, 96.0),
                    48.0,
                    &Shadow::new(
                        Color::from_hex(VIOLET).with_alpha((0.40 + 0.10 * pulse as f32) * a),
                        Vec2::new(-18.0, 0.0),
                        54.0,
                    ),
                );
                draw_centered_text(
                    p,
                    "rugkit 0.1",
                    LOGICAL_W * 0.5,
                    200.0,
                    &TextStyle::new(98.0)
                        .weight(FontWeight::BLACK)
                        .letter_spacing(-4.0)
                        .color(Color::WHITE.with_alpha(a)),
                );
            },
        );

        draw_centered_text(
            p,
            "Build Rust desktop apps that feel alive.",
            LOGICAL_W * 0.5,
            336.0,
            &TextStyle::new(26.0)
                .weight(FontWeight::SEMIBOLD)
                .color(Color::from_hex(0xD8D4F5).with_alpha(a)),
        );

        let cta = Rect::from_xywh(LOGICAL_W * 0.5 - 210.0, 404.0, 420.0, 44.0);
        p.shadow(
            cta,
            999.0,
            &Shadow::new(Color::from_hex(CYAN).with_alpha(0.35 * a), Vec2::ZERO, 22.0),
        );
        p.fill_rect(
            cta,
            999.0,
            Brush::linear(
                Point::new(cta.x0, cta.y0),
                Point::new(cta.x1, cta.y0),
                [
                    (0.0, Color::from_hex(VIOLET).with_alpha(a)),
                    (1.0, Color::from_hex(CYAN).with_alpha(a)),
                ],
            ),
        );
        draw_centered_text(
            p,
            "github.com/Shadrous/rugkit  ·  MIT OR Apache-2.0",
            LOGICAL_W * 0.5,
            cta.y0 + 13.0,
            &TextStyle::new(15.0)
                .weight(FontWeight::BOLD)
                .color(Color::from_hex(0x090814).with_alpha(a)),
        );
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "docs/demo/rugkit_launch.mp4".to_string());
    if let Some(parent) = std::path::Path::new(&out_path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    set_theme(Theme::PLAYFUL_DARK.with_accent(Color::from_hex(CYAN)));

    // Build a real live rugkit::Ui Counter for Act II.
    let count = signal(0i32);
    let mut counter_ui = Ui::new(move || {
        let tone = move || match count.get() {
            0 => theme().ink,
            c if c > 0 => Color::from_hex(CYAN),
            _ => Color::from_hex(MAGENTA),
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
        .shadow_tint(|| Color::from_hex(CYAN))
    });
    let _ = PointerButton::Primary;
    counter_ui.event(&InputEvent::AppearanceChanged(Appearance::new(
        ColorScheme::Dark,
        Some(Color::from_hex(CYAN)),
    )));

    let mut bar_anims = [0.0; 5].map(Animated::new);
    let mut last_step = 0usize;

    // Beat-locked 124 BPM synthwave electronic soundtrack (kick + Am/F/C/G warm pad + neon arp).
    // Beat period = 60 / 124 = 0.483871s; bar = 1.935484s.
    let audio_expr = concat!(
        "aevalsrc='",
        // Sub-bass punchy 124 BPM kick (active from t=4.8s to t=39.5s)
        "((gte(t,4.8)*lte(t,39.5))*0.34*exp(-mod(t,0.483871)*13)*sin(2*PI*(52+85*exp(-mod(t,0.483871)*26))*t)",
        // Warm nocturnal synth pad (Am -> F -> C -> G every 2 bars = 3.871s)
        "+0.14*(sin(2*PI*(if(lt(mod(t,15.484),3.871),220,if(lt(mod(t,15.484),7.742),174.61,if(lt(mod(t,15.484),11.613),261.63,196))))*t)",
        "+0.65*sin(2*PI*(if(lt(mod(t,15.484),3.871),261.63,if(lt(mod(t,15.484),7.742),220,if(lt(mod(t,15.484),11.613),329.63,246.94))))*t)",
        "+0.55*sin(2*PI*(if(lt(mod(t,15.484),3.871),329.63,if(lt(mod(t,15.484),7.742),261.63,if(lt(mod(t,15.484),11.613),392,293.66))))*t))",
        // Crisp 8th-note neon arpeggio sparkle during flagship acts (t=11..37s)
        "+(gte(t,11.0)*lte(t,37.0))*0.09*exp(-mod(t,0.241935)*11)*sin(2*PI*(440+110*floor(mod(t/0.241935,4)))*t))",
        // Clean fade-in & fade-out envelope
        "*min(1,t/1.2)*min(1,max(0,(42.0-t)/1.8))",
        "':s=48000:d=42.0"
    );

    let mut child = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &format!("{OUT_W}x{OUT_H}"),
            "-r",
            &FPS.to_string(),
            "-i",
            "-",
            "-f",
            "lavfi",
            "-i",
            audio_expr,
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-preset",
            "medium",
            "-crf",
            "20",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-shortest",
            "-movflags",
            "+faststart",
            &out_path,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()?;

    let mut stdin = child.stdin.take().expect("ffmpeg stdin");
    let mut offscreen = OffscreenRenderer::new()?;
    let mut canvas = Canvas::new();

    println!(
        "Rendering {TOTAL_FRAMES} frames ({OUT_W}x{OUT_H} @ {FPS}fps, 124 BPM) with rugkit GPU…"
    );

    for frame in 0..TOTAL_FRAMES {
        let t = f64::from(frame) / f64::from(FPS);
        let light_mix = if (32.0..37.5).contains(&t) {
            let lt = t - 32.0;
            (smootherstep(0.9, 1.8, lt) * (1.0 - smootherstep(4.1, 4.9, lt))) as f32
        } else {
            0.0
        };

        // Act-tailored neon atmosphere colors & subtle camera parallax
        let (acc_a, acc_b) = if t < 11.0 {
            (Color::from_hex(CYAN), Color::from_hex(VIOLET))
        } else if t < 21.5 {
            (Color::from_hex(VIOLET), Color::from_hex(MINT))
        } else if t < 32.0 {
            (Color::from_hex(CYAN), Color::from_hex(MAGENTA))
        } else {
            (Color::from_hex(VIOLET), Color::from_hex(CYAN))
        };
        let parallax = Vec2::new((t * 0.45).sin() * 32.0, (t * 0.3).cos() * 14.0);

        {
            let mut p = canvas.begin_frame(SCALE);
            draw_neon_scenery(&mut p, t, light_mix, parallax, acc_a, acc_b);

            if t < 5.0 {
                draw_act_intro(&mut p, t, t);
            } else if t < 11.0 {
                draw_act_builder(&mut p, t - 5.0, &mut counter_ui, count, &mut last_step);
            } else if t < 21.5 {
                draw_act_minty(&mut p, t - 11.0, &mut bar_anims);
            } else if t < 32.0 {
                draw_act_studio(&mut p, t - 21.5, t);
            } else if t < 37.5 {
                let _ = draw_act_system(&mut p, t - 32.0);
            } else {
                draw_act_finale(&mut p, t - 37.5, t);
            }
        }

        let rgba = offscreen.render_to_rgba(&canvas, OUT_W, OUT_H, Color::from_hex(0x04030A))?;
        stdin.write_all(&rgba)?;

        if frame % 300 == 0 {
            println!("  frame {frame}/{TOTAL_FRAMES} ({t:.1}s)");
        }
    }

    drop(stdin);
    let status = child.wait()?;
    if !status.success() {
        return Err(format!("ffmpeg exited with {status}").into());
    }
    println!("Done! Wrote {out_path}");
    Ok(())
}
