//! **Pivon on Bead** — 38-second 1080p 60fps launch motion video for `rugkit`.
//!
//! Rendered 100% natively on the GPU using rugkit's own [`OffscreenRenderer`],
//! [`Canvas`], [`Painter`], [`Spring`], and [`Transform`], piped directly into
//! `ffmpeg` with a warm, low-register ambient electronic soundtrack (no harsh
//! high-frequency synth lasers).
//!
//! Run:
//! ```bash
//! cargo run --release --example render_promo -- docs/demo/rugkit_launch.mp4
//! ```

use std::f64::consts::TAU;
use std::io::Write;
use std::process::{Command, Stdio};

use rugkit::prelude::*;
use rugkit::{Canvas, OffscreenRenderer, Transform};

const OUT_W: u32 = 1920;
const OUT_H: u32 = 1080;
const SCALE: f64 = 1.5; // 1280x720 logical -> 1920x1080 physical
const LOGICAL_W: f64 = OUT_W as f64 / SCALE;
const LOGICAL_H: f64 = OUT_H as f64 / SCALE;
const FPS: u32 = 60;
const DURATION_SECS: f64 = 38.0;
const TOTAL_FRAMES: u32 = (DURATION_SECS * FPS as f64) as u32;

// Exact Pivon on Bead design tokens.
const DESK_DARK_A: u32 = 0x3A3F4A;
const DESK_DARK_B: u32 = 0x23252B;
const DESK_LIGHT_A: u32 = 0xE4E0D6;
const DESK_LIGHT_B: u32 = 0xCFCABE;

const FILL_DARK: u32 = 0x0E0E10;
const WELL_DARK: u32 = 0x1C1C1F;
const WELL_HOVER_DARK: u32 = 0x27272B;
const INK_DARK: u32 = 0xF4F4F1;
const MUTED_DARK: u32 = 0xA1A19C;
const FAINT_DARK: u32 = 0x64645F;
const TIDE_DARK: u32 = 0x4CC3E8;

const FILL_LIGHT: u32 = 0xFAFAF8;
const WELL_LIGHT: u32 = 0xEEEEEA;
const WELL_HOVER_LIGHT: u32 = 0xE4E4DF;
const INK_LIGHT: u32 = 0x151514;
const MUTED_LIGHT: u32 = 0x5C5C57;
const TIDE_LIGHT: u32 = 0x0E7490;

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
    let c1 = 1.35;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

fn with_top_camera<R>(p: &mut Painter<'_>, zoom: f64, f: impl FnOnce(&mut Painter<'_>) -> R) -> R {
    let xform = Transform::scale_about(zoom, Point::new(LOGICAL_W * 0.5, 0.0));
    p.with_transform(xform, f)
}

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

fn draw_centered_text(p: &mut Painter<'_>, s: &str, cx: f64, y: f64, style: &TextStyle) {
    let m = p.measure_text(s, None, style);
    p.text(s, Point::new(cx - m.width * 0.5, y), style);
}

// ---- Pivon's living Pip & tactile cursor ------------------------------------------------

fn draw_pip_breathe(p: &mut Painter<'_>, c: Point, r: f64, color: Color, t: f64) {
    let s = 1.0 + 0.16 * (t * 1.55).sin().abs();
    p.fill_circle(c, r * s, color);
}

fn draw_pip_orbit(p: &mut Painter<'_>, c: Point, r: f64, color: Color, t: f64) {
    let mut arc = Path::new();
    let start = t * 6.8;
    for i in 0..=20 {
        let a = start + (i as f64 / 20.0) * (TAU * 0.72);
        let pt = Point::new(c.x + a.cos() * r, c.y + a.sin() * r);
        if i == 0 {
            arc.move_to(pt);
        } else {
            arc.line_to(pt);
        }
    }
    p.stroke_path(&arc, 2.0, color);
}

fn draw_pip_ripple(p: &mut Painter<'_>, c: Point, r: f64, color: Color, t: f64) {
    let phase = (t * 0.72).fract();
    let ring_r = r * (1.0 + phase * 1.65);
    let ring_a = ((1.0 - phase) as f32) * 0.55;
    p.stroke_rect(
        Rect::from_xywh(c.x - ring_r, c.y - ring_r, ring_r * 2.0, ring_r * 2.0),
        999.0,
        1.5,
        color.with_alpha(ring_a),
    );
    p.fill_circle(c, r, color);
}

fn draw_cursor(p: &mut Painter<'_>, pos: Point, click_age: Option<f64>, accent: Color, alpha: f32) {
    if alpha <= 0.01 {
        return;
    }
    if let Some(age) = click_age
        && (0.0..0.42).contains(&age)
    {
        let u = age / 0.42;
        let r = 6.0 + ease_out_cubic(u) * 20.0;
        let ra = ((1.0 - u) as f32) * 0.65 * alpha;
        p.stroke_rect(
            Rect::from_xywh(pos.x - r, pos.y - r, r * 2.0, r * 2.0),
            999.0,
            1.6,
            accent.with_alpha(ra),
        );
    }
    let mut arrow = Path::new();
    arrow.move_to(pos);
    arrow.line_to(Point::new(pos.x, pos.y + 18.0));
    arrow.line_to(Point::new(pos.x + 4.8, pos.y + 14.0));
    arrow.line_to(Point::new(pos.x + 8.6, pos.y + 20.4));
    arrow.line_to(Point::new(pos.x + 11.4, pos.y + 19.0));
    arrow.line_to(Point::new(pos.x + 7.8, pos.y + 12.6));
    arrow.line_to(Point::new(pos.x + 13.6, pos.y + 12.0));
    arrow.close();
    p.fill_path(&arrow, Color::WHITE.with_alpha(alpha));
    p.stroke_path(&arrow, 1.3, Color::from_hex(0x0E0E10).with_alpha(alpha));
}

/// Matte architectural Pivon desk background (Dark slate <-> Warm stone Light).
fn draw_desk(p: &mut Painter<'_>, light_mix: f32) {
    let a = Color::from_hex(DESK_DARK_A).mix(Color::from_hex(DESK_LIGHT_A), light_mix);
    let b = Color::from_hex(DESK_DARK_B).mix(Color::from_hex(DESK_LIGHT_B), light_mix);
    p.fill_rect(
        Rect::from_xywh(0.0, 0.0, LOGICAL_W, LOGICAL_H),
        0.0,
        Brush::linear(
            Point::new(LOGICAL_W * 0.25, 0.0),
            Point::new(LOGICAL_W * 0.75, LOGICAL_H),
            [(0.0, a), (1.0, b)],
        ),
    );
}

/// Restrained editorial caption at bottom-left, never competing with the UI.
fn draw_caption(p: &mut Painter<'_>, kicker: &str, title: &str, alpha: f32, light_mix: f32) {
    if alpha <= 0.01 {
        return;
    }
    let ink = Color::from_hex(0xECECE8).mix(Color::from_hex(0x2A2A27), light_mix);
    let muted = Color::from_hex(0xA9A9A4).mix(Color::from_hex(0x5A5852), light_mix);
    let tide = Color::from_hex(TIDE_DARK).mix(Color::from_hex(TIDE_LIGHT), light_mix);

    p.fill_circle(Point::new(48.0, 661.0), 4.0, tide.with_alpha(alpha));
    p.text(
        kicker,
        Point::new(62.0, 654.0),
        &TextStyle::new(11.5)
            .weight(FontWeight::SEMIBOLD)
            .letter_spacing(1.2)
            .color(muted.with_alpha(alpha)),
    );
    p.text(
        title,
        Point::new(44.0, 674.0),
        &TextStyle::new(18.0)
            .weight(FontWeight::SEMIBOLD)
            .letter_spacing(-0.2)
            .color(ink.with_alpha(alpha)),
    );
}

/// Draws the matte-black Notch Island fused to the top edge of the display frame.
fn draw_notch_shell(p: &mut Painter<'_>, cx: f64, top_y: f64, w: f64, h: f64, rad: f64) -> Rect {
    let r = Rect::from_xywh(cx - w * 0.5, top_y, w, h);
    if h > 42.0 {
        p.shadow(
            r,
            rad,
            &Shadow::new(Color::rgba(0.0, 0.0, 0.0, 0.48), Vec2::new(0.0, 22.0), 50.0),
        );
    }
    p.fill_rect(r, rad, Color::BLACK);
    p.fill_rect(
        Rect::from_xywh(r.x0, r.y0, r.width(), rad.min(18.0)),
        0.0,
        Color::BLACK,
    );
    r
}

// ---- ACT 1 (0.0s .. 10.5s): RESTING -> PEEK -> SUMMON -> IN-PLACE OPTIONS -> WORKING ----

fn draw_act_island_lifecycle(p: &mut Painter<'_>, t: f64) {
    let well = Color::from_hex(WELL_DARK);
    let well_h = Color::from_hex(WELL_HOVER_DARK);
    let ink = Color::from_hex(INK_DARK);
    let muted = Color::from_hex(MUTED_DARK);
    let tide = Color::from_hex(TIDE_DARK);

    // Subtle background app silhouette on the desk (just like Pivon board #1)
    let desk_card = Rect::from_xywh(160.0, 128.0, 960.0, 592.0);
    p.fill_rect(desk_card, 18.0, Color::rgba(1.0, 1.0, 1.0, 0.04));
    p.stroke_rect(desk_card, 18.0, 1.0, Color::rgba(1.0, 1.0, 1.0, 0.06));

    // Opening title fades smoothly out as the island opens
    if t < 3.4 {
        let ta = (smoothstep(0.0, 0.5, t) * (1.0 - smoothstep(2.5, 3.3, t))) as f32;
        draw_centered_text(
            p,
            "rugkit  ·  Pivon on Bead",
            LOGICAL_W * 0.5,
            274.0,
            &TextStyle::new(48.0)
                .weight(FontWeight::SEMIBOLD)
                .letter_spacing(-1.6)
                .color(Color::from_hex(0xF4F4F1).with_alpha(ta)),
        );
        draw_centered_text(
            p,
            "One shape. It grows when you call it, and folds back into the notch when you move on.",
            LOGICAL_W * 0.5,
            340.0,
            &TextStyle::new(18.0).color(Color::from_hex(0xA9A9A4).with_alpha(ta)),
        );
    }

    // Smoothly morph island dimensions across:
    //   0.0..1.5s : Resting (210 x 34)
    //   1.5..2.8s : Peek (342 x 34)
    //   2.8..5.0s : Summon (420 x 338)
    //   5.0..7.2s : Summon + In-place options (420 x 424)
    //   7.2..8.9s : Thinking (356 x 34)
    //   8.9..10.5s: Working (368 x 34)
    let u_peek = smootherstep(1.3, 1.85, t);
    let u_summon = smootherstep(2.65, 3.25, t);
    let u_opts = smootherstep(4.95, 5.45, t);
    let u_retract = smootherstep(7.05, 7.60, t);
    let u_work = smootherstep(8.75, 9.15, t);

    let mut w = lerp(210.0, 342.0, u_peek);
    w = lerp(w, 420.0, u_summon);
    w = lerp(w, 356.0, u_retract);
    w = lerp(w, 368.0, u_work);

    let mut h = lerp(34.0, 338.0, u_summon);
    h = lerp(h, 424.0, u_opts);
    h = lerp(h, 34.0, u_retract);

    let rad = lerp(16.0, 28.0, u_summon * (1.0 - u_retract));

    // Gentle cinema camera anchored to the top bezel so the notch stays fused to the top edge!
    let zoom = lerp(
        1.0,
        1.24,
        smootherstep(2.7, 3.6, t) * (1.0 - smootherstep(7.0, 7.9, t)),
    ) + lerp(0.0, 0.28, smootherstep(7.6, 8.6, t));

    with_top_camera(p, zoom, |p| {
        let island = draw_notch_shell(p, LOGICAL_W * 0.5, 0.0, w, h, rad);

        // Top wings bar (always present inside the notch!)
        if u_retract < 0.5 {
            draw_pip_breathe(p, Point::new(island.x0 + 18.0, 17.0), 4.0, tide, t);
            let wing_a = u_peek as f32;
            if wing_a > 0.02 {
                p.text(
                    "Ready",
                    Point::new(island.x0 + 30.0, 9.0),
                    &TextStyle::new(13.0)
                        .weight(FontWeight::MEDIUM)
                        .color(ink.with_alpha(wing_a)),
                );
                p.text(
                    "Qwen3.5 4B",
                    Point::new(island.x1 - 88.0, 9.0),
                    &TextStyle::new(13.0)
                        .weight(FontWeight::MEDIUM)
                        .color(muted.with_alpha(wing_a)),
                );
            }
        } else if t < 8.9 {
            // Thinking: orbiting Tide ring + live seconds counter
            let ta = smoothstep(7.35, 7.65, t) as f32;
            draw_pip_orbit(
                p,
                Point::new(island.x0 + 19.0, 17.0),
                5.0,
                tide.with_alpha(ta),
                t,
            );
            p.text(
                "Thinking",
                Point::new(island.x0 + 32.0, 9.0),
                &TextStyle::new(13.0)
                    .weight(FontWeight::MEDIUM)
                    .color(ink.with_alpha(ta)),
            );
            let secs = (((t - 7.3) * 3.8).clamp(1.0, 6.0)) as u32;
            p.text(
                &format!("{secs} s"),
                Point::new(island.x1 - 36.0, 9.0),
                &TextStyle::new(13.0)
                    .weight(FontWeight::SEMIBOLD)
                    .color(muted.with_alpha(ta)),
            );
        } else {
            // Working: rippling pip + "Searching the web" + "2 of 4" -> "4 of 4" + 2px Tide progress rim
            let wa = smoothstep(8.9, 9.15, t) as f32;
            draw_pip_ripple(
                p,
                Point::new(island.x0 + 19.0, 17.0),
                4.0,
                tide.with_alpha(wa),
                t,
            );
            p.text(
                "Searching the web",
                Point::new(island.x0 + 32.0, 9.0),
                &TextStyle::new(13.0)
                    .weight(FontWeight::MEDIUM)
                    .color(ink.with_alpha(wa)),
            );
            let prog = clamp01((t - 8.9) / 1.55);
            let step = ((prog * 4.0).floor() as usize).min(3) + 1;
            p.text(
                &format!("{step} of 4"),
                Point::new(island.x1 - 52.0, 9.0),
                &TextStyle::new(13.0)
                    .weight(FontWeight::SEMIBOLD)
                    .color(muted.with_alpha(wa)),
            );
            p.fill_rect(
                Rect::from_xywh(
                    island.x0 + 10.0,
                    island.y1 - 2.5,
                    (island.width() - 20.0) * prog,
                    2.5,
                ),
                2.0,
                tide.with_alpha(wa),
            );
        }

        // Summon Composer interior (visible while open)
        let body_a = (smoothstep(2.9, 3.3, t) * (1.0 - smoothstep(7.0, 7.35, t))) as f32;
        if body_a > 0.02 {
            p.with_clip(island, rad, |p| {
                p.with_opacity(body_a, island, |p| {
                    let comp_h = lerp(98.0, 184.0, u_opts);
                    let comp =
                        Rect::from_xywh(island.x0 + 12.0, 42.0, island.width() - 24.0, comp_h);
                    p.fill_rect(comp, 16.0, well);

                    // Live typing into composer well
                    let prompt = "Plan a relaxed beach weekend in Burgas with two kids";
                    let n = (((t - 3.35) * 34.0).clamp(0.0, prompt.len() as f64)) as usize;
                    let shown = if n == 0 { "Ask anything" } else { &prompt[..n] };
                    let text_col = if n == 0 { muted } else { ink };
                    p.text(
                        shown,
                        Point::new(comp.x0 + 14.0, comp.y0 + 12.0),
                        &TextStyle::new(14.0).color(text_col),
                    );

                    // In-place options drawer (grows organically inside the well!)
                    if u_opts > 0.05 {
                        let ob = Rect::from_xywh(
                            comp.x0 + 10.0,
                            comp.y0 + 44.0,
                            comp.width() - 20.0,
                            82.0 * u_opts,
                        );
                        p.with_clip(ob, 12.0, |p| {
                            p.fill_rect(ob, 12.0, well_h);
                            let draw_seg = |p: &mut Painter<'_>,
                                            y: f64,
                                            lbl: &str,
                                            opts: [&str; 3],
                                            sel: usize| {
                                p.text(
                                    lbl,
                                    Point::new(ob.x0 + 10.0, y + 6.0),
                                    &TextStyle::new(12.0).weight(FontWeight::MEDIUM).color(muted),
                                );
                                let track =
                                    Rect::from_xywh(ob.x0 + 74.0, y, ob.width() - 84.0, 28.0);
                                p.fill_rect(track, 999.0, Color::BLACK);
                                let sw = (track.width() - 4.0) / 3.0;
                                for (i, &opt) in opts.iter().enumerate() {
                                    let sr = Rect::from_xywh(
                                        track.x0 + 2.0 + i as f64 * sw,
                                        track.y0 + 2.0,
                                        sw,
                                        24.0,
                                    );
                                    if i == sel {
                                        p.fill_rect(sr, 999.0, ink);
                                    }
                                    draw_centered_text(
                                        p,
                                        opt,
                                        sr.center().x,
                                        sr.y0 + 4.5,
                                        &TextStyle::new(12.0)
                                            .weight(FontWeight::SEMIBOLD)
                                            .color(if i == sel { Color::BLACK } else { muted }),
                                    );
                                }
                            };
                            draw_seg(p, ob.y0 + 8.0, "Thinking", ["Off", "Auto", "On"], 1);
                            draw_seg(
                                p,
                                ob.y0 + 44.0,
                                "Depth",
                                ["Quick", "Balanced", "Deep"],
                                if t > 5.8 { 2 } else { 1 },
                            );
                        });
                    }

                    // Bottom chips + solid Send button
                    let cy = comp.y1 - 36.0;
                    p.fill_rect(
                        Rect::from_xywh(comp.x0 + 12.0, cy, 62.0, 28.0),
                        999.0,
                        well_h,
                    );
                    p.text(
                        "Auto",
                        Point::new(comp.x0 + 28.0, cy + 6.5),
                        &TextStyle::new(12.0).weight(FontWeight::SEMIBOLD).color(ink),
                    );
                    let web_on = t > 6.25;
                    p.fill_rect(
                        Rect::from_xywh(comp.x0 + 80.0, cy, 54.0, 28.0),
                        999.0,
                        if web_on { well_h } else { Color::TRANSPARENT },
                    );
                    p.text(
                        "Web",
                        Point::new(comp.x0 + 94.0, cy + 6.5),
                        &TextStyle::new(12.0)
                            .weight(FontWeight::SEMIBOLD)
                            .color(if web_on { ink } else { muted }),
                    );
                    p.fill_circle(Point::new(comp.x1 - 24.0, cy + 14.0), 15.0, ink);
                    p.text(
                        "↑",
                        Point::new(comp.x1 - 28.5, cy + 5.0),
                        &TextStyle::new(14.0)
                            .weight(FontWeight::BOLD)
                            .color(Color::BLACK),
                    );

                    // Recent chats list
                    let rec_y = comp.y1 + 14.0;
                    p.text(
                        "Recent",
                        Point::new(island.x0 + 18.0, rec_y),
                        &TextStyle::new(12.0).weight(FontWeight::MEDIUM).color(muted),
                    );
                    let recents = [
                        ("Weekend trip to Burgas", "2 h", true),
                        ("Fix the backup script", "Yesterday", false),
                        ("Turkish lesson plan", "Mon", false),
                    ];
                    for (i, (title, age, hl)) in recents.into_iter().enumerate() {
                        let ry = rec_y + 22.0 + i as f64 * 40.0;
                        let rr = Rect::from_xywh(island.x0 + 12.0, ry, island.width() - 24.0, 36.0);
                        if hl {
                            p.fill_rect(rr, 12.0, well);
                        }
                        p.text(
                            title,
                            Point::new(rr.x0 + 12.0, rr.y0 + 9.5),
                            &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(ink),
                        );
                        p.text(
                            age,
                            Point::new(rr.x1 - 64.0, rr.y0 + 10.0),
                            &TextStyle::new(12.0).color(muted),
                        );
                    }
                });
            });
        }

        // Tactile cursor gliding to notch -> clicking -> opening Auto -> toggling Deep -> Web -> Send
        let p_notch = Point::new(LOGICAL_W * 0.5, 18.0);
        let p_auto = Point::new(LOGICAL_W * 0.5 - 156.0, 118.0);
        let p_deep = Point::new(LOGICAL_W * 0.5 + 126.0, 146.0);
        let p_web = Point::new(LOGICAL_W * 0.5 - 92.0, 204.0);
        let p_send = Point::new(LOGICAL_W * 0.5 + 174.0, 204.0);

        let cur = if t < 2.9 {
            let u = smootherstep(0.9, 1.5, t);
            Point::new(
                lerp(LOGICAL_W * 0.5 + 90.0, p_notch.x, u),
                lerp(140.0, p_notch.y, u),
            )
        } else if t < 5.2 {
            let u = smootherstep(4.3, 4.9, t);
            Point::new(lerp(p_notch.x, p_auto.x, u), lerp(p_notch.y, p_auto.y, u))
        } else if t < 6.0 {
            let u = smootherstep(5.4, 5.75, t);
            Point::new(lerp(p_auto.x, p_deep.x, u), lerp(p_auto.y, p_deep.y, u))
        } else if t < 6.5 {
            let u = smootherstep(6.0, 6.22, t);
            Point::new(lerp(p_deep.x, p_web.x, u), lerp(p_deep.y, p_web.y, u))
        } else {
            let u = smootherstep(6.5, 6.95, t);
            Point::new(lerp(p_web.x, p_send.x, u), lerp(p_web.y, p_send.y, u))
        };
        let clicks = [2.65, 4.95, 5.80, 6.25, 7.0];
        let click_age = clicks
            .iter()
            .filter(|&&c| t >= c && t - c < 0.42)
            .map(|&c| t - c)
            .next_back();
        let cur_a = (smoothstep(0.8, 1.1, t) * (1.0 - smoothstep(7.4, 7.8, t))) as f32;
        draw_cursor(p, cur, click_age, tide, cur_a);
    });

    let (kicker, headline) = if t < 2.7 {
        ("THE NOTCH ISLAND", "Resting & Peek · One point of light")
    } else if t < 7.1 {
        (
            "SUMMON & IN-PLACE OPTIONS",
            "Options grow inside the well — nothing floats over it",
        )
    } else if t < 8.9 {
        ("THINKING", "An orbiting Tide ring and live seconds counter")
    } else {
        ("WORKING", "Rippling pip, step count, and 2px progress rim")
    };
    draw_caption(p, kicker, headline, smoothstep(1.2, 1.7, t) as f32, 0.0);
}

// ---- ACT 2 (10.5s .. 15.0s): NEEDS YOU — COMMAND APPROVAL & MEMORY REVIEW ---------------

fn draw_act_approvals(p: &mut Painter<'_>, lt: f64) {
    let well = Color::from_hex(WELL_DARK);
    let ink = Color::from_hex(INK_DARK);
    let muted = Color::from_hex(MUTED_DARK);
    let tide = Color::from_hex(TIDE_DARK);

    let is_memory = lt >= 2.35;
    let open_t = ease_out_back(lt / 0.55);
    let h = lerp(34.0, 206.0, open_t);
    let w = lerp(350.0, 396.0, open_t);

    // Subtle single nudge right before opening (exact Pivon spec!)
    let nudge = (smoothstep(0.0, 0.15, lt) * (1.0 - smoothstep(0.15, 0.35, lt))) * 5.0;

    with_top_camera(p, 1.32, |p| {
        let island = draw_notch_shell(p, LOGICAL_W * 0.5 + nudge, 0.0, w, h, 28.0);
        p.with_clip(island, 28.0, |p| {
            p.fill_circle(Point::new(island.x0 + 20.0, 17.0), 6.0, tide);
            p.text(
                if is_memory {
                    "Remember this?"
                } else {
                    "Needs you"
                },
                Point::new(island.x0 + 34.0, 9.0),
                &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(ink),
            );
            p.text(
                if is_memory { "2" } else { "1" },
                Point::new(island.x1 - 26.0, 9.0),
                &TextStyle::new(13.0)
                    .weight(FontWeight::SEMIBOLD)
                    .color(muted),
            );

            p.text(
                if is_memory {
                    "Remember that you want Celsius?"
                } else {
                    "Run this command in your workspace?"
                },
                Point::new(island.x0 + 20.0, 46.0),
                &TextStyle::new(15.0).weight(FontWeight::SEMIBOLD).color(ink),
            );
            p.text(
                if is_memory {
                    "You said it while planning the trip. I'll keep it for every chat."
                } else {
                    "I want to list the reports folder to find last month's file."
                },
                Point::new(island.x0 + 20.0, 70.0),
                &TextStyle::new(12.5).color(muted),
            );

            let box_r = Rect::from_xywh(island.x0 + 14.0, 100.0, island.width() - 28.0, 44.0);
            p.fill_rect(box_r, 14.0, well);
            p.text(
                if is_memory {
                    "“Always give me temperatures in Celsius.”"
                } else {
                    "ls -la reports/"
                },
                Point::new(box_r.x0 + 14.0, box_r.y0 + 13.0),
                &TextStyle::new(13.0)
                    .family(if is_memory {
                        FontFamily::Ui
                    } else {
                        FontFamily::Monospace
                    })
                    .color(ink),
            );

            let by = island.y1 - 44.0;
            if is_memory {
                p.text(
                    "1 more to review",
                    Point::new(island.x0 + 20.0, by + 8.0),
                    &TextStyle::new(12.0).color(muted),
                );
            }
            let q = Rect::from_xywh(island.x1 - 186.0, by, 78.0, 32.0);
            p.fill_rect(q, 999.0, well);
            draw_centered_text(
                p,
                "Not now",
                q.center().x,
                q.y0 + 7.5,
                &TextStyle::new(12.5).weight(FontWeight::SEMIBOLD).color(ink),
            );

            let s = Rect::from_xywh(island.x1 - 98.0, by, 84.0, 32.0);
            p.fill_rect(s, 999.0, ink);
            draw_centered_text(
                p,
                if is_memory { "Remember" } else { "Run" },
                s.center().x,
                s.y0 + 7.5,
                &TextStyle::new(12.5)
                    .weight(FontWeight::SEMIBOLD)
                    .color(Color::BLACK),
            );
        });

        let btn_c = Point::new(island.x1 - 56.0, island.y1 - 28.0);
        let cur = Point::new(
            lerp(
                island.center().x - 60.0,
                btn_c.x,
                smootherstep(0.6, 1.5, lt),
            ),
            lerp(island.y1 + 30.0, btn_c.y, smootherstep(0.6, 1.5, lt)),
        );
        let clicks = [2.15, 4.15];
        let click_age = clicks
            .iter()
            .filter(|&&c| lt >= c && lt - c < 0.42)
            .map(|&c| lt - c)
            .next_back();
        draw_cursor(p, cur, click_age, tide, 1.0);
    });

    draw_caption(
        p,
        "NEEDS YOU · APPROVALS",
        if is_memory {
            "Memory review · Your exact words, never a paraphrase"
        } else {
            "Command approval · Exactly what will run, in a well"
        },
        1.0,
        0.0,
    );
}

// ---- ACT 3 (15.0s .. 21.5s): CONVERSATION STAGE (STREAMING + CITATIONS + SOURCE PILLS) --

fn draw_act_conversation(p: &mut Painter<'_>, lt: f64, global_t: f64) {
    let well = Color::from_hex(WELL_DARK);
    let well_h = Color::from_hex(WELL_HOVER_DARK);
    let ink = Color::from_hex(INK_DARK);
    let muted = Color::from_hex(MUTED_DARK);
    let tide = Color::from_hex(TIDE_DARK);

    let open = smootherstep(0.0, 0.75, lt);
    let w = lerp(396.0, 444.0, open);
    let h = lerp(206.0, 610.0, open);

    let zoom = lerp(1.12, 1.06, smootherstep(0.0, 5.0, lt));

    with_top_camera(p, zoom, |p| {
        let island = draw_notch_shell(p, LOGICAL_W * 0.5, 0.0, w, h, 32.0);
        p.with_clip(island, 32.0, |p| {
            draw_pip_ripple(p, Point::new(island.x0 + 20.0, 17.0), 4.0, tide, global_t);
            p.text(
                "Answering",
                Point::new(island.x0 + 32.0, 9.0),
                &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(ink),
            );
            p.text(
                "18%",
                Point::new(island.x1 - 42.0, 9.0),
                &TextStyle::new(13.0)
                    .weight(FontWeight::SEMIBOLD)
                    .color(muted),
            );

            // Header with "Open in window" icon button
            p.text(
                "Weekend trip to Burgas",
                Point::new(island.x0 + 20.0, 44.0),
                &TextStyle::new(15.0).weight(FontWeight::SEMIBOLD).color(ink),
            );
            let win_btn = Rect::from_xywh(island.x1 - 42.0, 38.0, 28.0, 28.0);
            if lt > 5.4 {
                p.fill_rect(win_btn, 999.0, well_h);
            }
            p.stroke_rect(
                Rect::from_xywh(win_btn.x0 + 7.0, win_btn.y0 + 8.0, 14.0, 12.0),
                3.0,
                1.5,
                ink,
            );
            p.stroke_line(
                Point::new(win_btn.x0 + 12.0, win_btn.y0 + 8.0),
                Point::new(win_btn.x0 + 12.0, win_btn.y0 + 20.0),
                1.5,
                ink,
            );

            // User bubble 1
            let ub1 = Rect::from_xywh(island.x0 + 84.0, 78.0, island.width() - 104.0, 52.0);
            p.fill_rect(ub1, 16.0, well_h);
            p.text(
                "Plan a relaxed beach weekend in Burgas",
                Point::new(ub1.x0 + 14.0, ub1.y0 + 10.0),
                &TextStyle::new(13.5).color(ink),
            );
            p.text(
                "with two kids, late May.",
                Point::new(ub1.x0 + 14.0, ub1.y0 + 28.0),
                &TextStyle::new(13.5).color(ink),
            );

            // Reasoning & web search trace
            p.fill_circle(
                Point::new(island.x0 + 24.0, 150.0),
                3.0,
                Color::from_hex(FAINT_DARK),
            );
            p.text(
                "Thought for 6 s",
                Point::new(island.x0 + 34.0, 143.0),
                &TextStyle::new(12.0).color(muted),
            );
            p.text(
                "✓  Searched the web · 4 sources",
                Point::new(island.x0 + 20.0, 164.0),
                &TextStyle::new(12.0).color(tide),
            );

            let lines = [
                "Late May is warm enough for the beach and before",
                "the summer crowds. A plan that keeps days short:",
                "• Saturday: Central Beach & Sea Garden [1]",
                "• Sunday: boat to St. Anastasia Island [2]",
                "• Book the boat ahead; weekend sailings fill [2]",
            ];
            for (i, ln) in lines.iter().enumerate() {
                p.text(
                    ln,
                    Point::new(island.x0 + 20.0, 194.0 + i as f64 * 22.0),
                    &TextStyle::new(13.5).color(ink),
                );
            }

            // Source pills
            p.fill_rect(
                Rect::from_xywh(island.x0 + 20.0, 314.0, 114.0, 26.0),
                999.0,
                well,
            );
            p.text(
                "1  visitburgas.bg",
                Point::new(island.x0 + 30.0, 320.0),
                &TextStyle::new(12.0).color(ink),
            );
            p.fill_rect(
                Rect::from_xywh(island.x0 + 142.0, 314.0, 116.0, 26.0),
                999.0,
                well,
            );
            p.text(
                "2  burgasport.bg",
                Point::new(island.x0 + 152.0, 320.0),
                &TextStyle::new(12.0).color(ink),
            );

            // User follow-up bubble
            if lt > 1.3 {
                let fa = smoothstep(1.3, 1.7, lt) as f32;
                let ub2 = Rect::from_xywh(island.x0 + 132.0, 362.0, island.width() - 152.0, 38.0);
                p.fill_rect(ub2, 16.0, well_h.with_alpha(fa));
                p.text(
                    "Is the sea warm enough to swim then?",
                    Point::new(ub2.x0 + 14.0, ub2.y0 + 10.0),
                    &TextStyle::new(13.5).color(ink.with_alpha(fa)),
                );
            }

            // Live token-by-token streaming response with Tide caret!
            if lt > 1.8 {
                let full_a = "Still cool for long swims. Most families paddle";
                let full_b = "at the edge; a light wetsuit helps kids stay longer";
                let chars = (((lt - 1.8) * 32.0) as usize).min(full_a.len() + full_b.len());
                let a_n = chars.min(full_a.len());
                let b_n = chars.saturating_sub(full_a.len());
                p.text(
                    &full_a[..a_n],
                    Point::new(island.x0 + 20.0, 418.0),
                    &TextStyle::new(13.5).color(ink),
                );
                if b_n > 0 {
                    p.text(
                        &full_b[..b_n],
                        Point::new(island.x0 + 20.0, 440.0),
                        &TextStyle::new(13.5).color(ink),
                    );
                }
                let (cx, cy) = if b_n > 0 {
                    (
                        island.x0
                            + 24.0
                            + p.measure_text(&full_b[..b_n], None, &TextStyle::new(13.5))
                                .width,
                        442.0,
                    )
                } else {
                    (
                        island.x0
                            + 24.0
                            + p.measure_text(&full_a[..a_n], None, &TextStyle::new(13.5))
                                .width,
                        420.0,
                    )
                };
                p.fill_rect(Rect::from_xywh(cx, cy, 7.0, 14.0), 2.0, tide);
            }

            // Bottom reply composer well
            let rep = Rect::from_xywh(
                island.x0 + 12.0,
                island.y1 - 84.0,
                island.width() - 24.0,
                72.0,
            );
            p.fill_rect(rep, 16.0, well);
            p.text(
                "Reply",
                Point::new(rep.x0 + 14.0, rep.y0 + 10.0),
                &TextStyle::new(13.5).color(muted),
            );
            p.fill_rect(
                Rect::from_xywh(rep.x0 + 12.0, rep.y1 - 34.0, 56.0, 26.0),
                999.0,
                well_h,
            );
            p.text(
                "Auto",
                Point::new(rep.x0 + 24.0, rep.y1 - 28.0),
                &TextStyle::new(12.0).weight(FontWeight::SEMIBOLD).color(ink),
            );
            p.fill_rect(
                Rect::from_xywh(rep.x0 + 74.0, rep.y1 - 34.0, 52.0, 26.0),
                999.0,
                well_h,
            );
            p.text(
                "Web",
                Point::new(rep.x0 + 86.0, rep.y1 - 28.0),
                &TextStyle::new(12.0).weight(FontWeight::SEMIBOLD).color(ink),
            );
            p.fill_circle(Point::new(rep.x1 - 22.0, rep.y1 - 21.0), 14.0, well_h);
            p.fill_rect(
                Rect::from_xywh(rep.x1 - 26.0, rep.y1 - 25.0, 8.0, 8.0),
                2.0,
                ink,
            );
        });

        // Cursor glides to "Open in window" icon at lt = 5.1..5.95s
        if lt > 4.6 {
            let u = smootherstep(4.6, 5.7, lt);
            let target = Point::new(island.x1 - 28.0, 52.0);
            let cur = Point::new(
                lerp(island.center().x + 40.0, target.x, u),
                lerp(400.0, target.y, u),
            );
            let click_age = (5.85..6.27).contains(&lt).then_some(lt - 5.85);
            draw_cursor(p, cur, click_age, tide, smoothstep(4.6, 4.9, lt) as f32);
        }
    });

    draw_caption(
        p,
        "CONVERSATION STAGE",
        "Streams directly from the notch with inline citations and sources",
        1.0,
        0.0,
    );
}

// ---- ACT 4 (21.5s .. 28.5s): PIVON WINDOW (HISTORY -> SETTINGS + LIGHT MORPH + ? HINT) --

fn draw_act_window_and_settings(p: &mut Painter<'_>, lt: f64, global_t: f64) -> f32 {
    // 0.0..3.2s : Pivon History Window in Dark finish
    // 3.2..7.0s : Settings · Advanced morphing Dark -> Warm Stone Light + "?" Hint Tooltip!
    let is_settings = lt >= 3.2;
    let light_mix = smootherstep(3.4, 4.4, lt) as f32;

    let fill = Color::from_hex(FILL_DARK).mix(Color::from_hex(FILL_LIGHT), light_mix);
    let well = Color::from_hex(WELL_DARK).mix(Color::from_hex(WELL_LIGHT), light_mix);
    let well_h = Color::from_hex(WELL_HOVER_DARK).mix(Color::from_hex(WELL_HOVER_LIGHT), light_mix);
    let rim = Color::rgba(1.0, 1.0, 1.0, 0.09).mix(Color::rgba(0.0, 0.0, 0.0, 0.07), light_mix);
    let ink = Color::from_hex(INK_DARK).mix(Color::from_hex(INK_LIGHT), light_mix);
    let muted = Color::from_hex(MUTED_DARK).mix(Color::from_hex(MUTED_LIGHT), light_mix);
    let tide = Color::from_hex(TIDE_DARK).mix(Color::from_hex(TIDE_LIGHT), light_mix);

    let open = ease_out_back(lt / 0.55);
    let win = Rect::from_xywh(140.0, 48.0, 1000.0, 576.0);

    with_camera(p, win.center(), lerp(0.92, 1.0, open), |p| {
        p.shadow(
            win,
            14.0,
            &Shadow::new(
                Color::rgba(0.0, 0.0, 0.0, 0.48 - 0.28 * light_mix),
                Vec2::new(0.0, 22.0),
                52.0,
            ),
        );
        p.fill_rect(win, 14.0, fill);
        p.stroke_rect(win, 14.0, 1.0, rim);

        // macOS traffic lights
        for (i, hex) in [0xFF5F57u32, 0xFEBC2E, 0x28C840].into_iter().enumerate() {
            p.fill_circle(
                Point::new(win.x0 + 22.0 + i as f64 * 18.0, win.y0 + 22.0),
                5.5,
                Color::from_hex(hex),
            );
        }

        if !is_settings {
            // ===== PIVON HISTORY WINDOW =====
            let sb_w = 256.0;
            p.stroke_line(
                Point::new(win.x0 + sb_w, win.y0),
                Point::new(win.x0 + sb_w, win.y1),
                1.0,
                rim,
            );
            let sr = Rect::from_xywh(win.x0 + 12.0, win.y0 + 44.0, sb_w - 60.0, 32.0);
            p.fill_rect(sr, 10.0, well);
            p.text(
                "Search",
                Point::new(sr.x0 + 12.0, sr.y0 + 8.0),
                &TextStyle::new(13.0).color(muted),
            );
            let add = Rect::from_xywh(sr.x1 + 6.0, sr.y0, 32.0, 32.0);
            p.fill_rect(add, 999.0, well);
            p.text(
                "+",
                Point::new(add.x0 + 11.0, add.y0 + 6.0),
                &TextStyle::new(16.0).color(ink),
            );

            let items: [(&str, Option<&str>); 7] = [
                ("Weekend trip to Burgas", Some("Today")),
                ("Compare two contracts", None),
                ("Fix the backup script", Some("Yesterday")),
                ("Summarize meeting notes", None),
                ("Turkish lesson plan", Some("Earlier")),
                ("Router port forwarding", None),
                ("Sourdough schedule", None),
            ];
            let mut y = win.y0 + 92.0;
            for (i, (title, grp)) in items.into_iter().enumerate() {
                if let Some(g) = grp {
                    p.text(
                        g,
                        Point::new(win.x0 + 20.0, y),
                        &TextStyle::new(12.0).weight(FontWeight::MEDIUM).color(muted),
                    );
                    y += 22.0;
                }
                let rr = Rect::from_xywh(win.x0 + 12.0, y, sb_w - 24.0, 32.0);
                if i == 0 {
                    p.fill_rect(rr, 10.0, well_h);
                }
                p.text(
                    title,
                    Point::new(rr.x0 + 10.0, rr.y0 + 7.5),
                    &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(ink),
                );
                y += 34.0;
            }

            // Model footer with Settings gear button
            draw_pip_breathe(
                p,
                Point::new(win.x0 + 24.0, win.y1 - 28.0),
                4.0,
                tide,
                global_t,
            );
            p.text(
                "Qwen3.5 4B",
                Point::new(win.x0 + 38.0, win.y1 - 42.0),
                &TextStyle::new(13.0).weight(FontWeight::SEMIBOLD).color(ink),
            );
            p.text(
                "Ready",
                Point::new(win.x0 + 38.0, win.y1 - 24.0),
                &TextStyle::new(12.0).color(muted),
            );
            let gear_c = Point::new(win.x0 + sb_w - 24.0, win.y1 - 28.0);
            p.fill_circle(
                gear_c,
                13.0,
                if lt > 2.6 { well_h } else { Color::TRANSPARENT },
            );
            p.stroke_rect(
                Rect::from_xywh(gear_c.x - 5.0, gear_c.y - 5.0, 10.0, 10.0),
                999.0,
                1.5,
                muted,
            );

            // Main conversation area
            let mx = win.x0 + sb_w;
            let mw = win.width() - sb_w;
            p.stroke_line(
                Point::new(mx, win.y0 + 52.0),
                Point::new(win.x1, win.y0 + 52.0),
                1.0,
                rim,
            );
            p.text(
                "Weekend trip to Burgas",
                Point::new(mx + 28.0, win.y0 + 16.0),
                &TextStyle::new(15.0).weight(FontWeight::SEMIBOLD).color(ink),
            );

            let col_w = 580.0;
            let cx0 = mx + (mw - col_w) * 0.5;
            let b1 = Rect::from_xywh(cx0 + col_w - 390.0, win.y0 + 74.0, 390.0, 42.0);
            p.fill_rect(b1, 16.0, well_h);
            p.text(
                "Plan a relaxed beach weekend in Burgas with two kids, late May.",
                Point::new(b1.x0 + 14.0, b1.y0 + 12.0),
                &TextStyle::new(13.0).color(ink),
            );
            p.text(
                "Thought for 6 s      Searched the web · 4 sources",
                Point::new(cx0, win.y0 + 132.0),
                &TextStyle::new(12.0).color(muted),
            );
            let body = [
                "Late May is warm enough for the beach and before the summer crowds:",
                "• Saturday: Central Beach in the morning, Sea Garden after lunch. [1]",
                "• Sunday: the boat to St. Anastasia Island, a short crossing. [2]",
                "• Book the boat a few days ahead; weekend sailings fill up. [2]",
            ];
            for (i, ln) in body.iter().enumerate() {
                p.text(
                    ln,
                    Point::new(cx0, win.y0 + 162.0 + i as f64 * 24.0),
                    &TextStyle::new(13.5).color(ink),
                );
            }
            let b2 = Rect::from_xywh(cx0 + col_w - 280.0, win.y0 + 278.0, 280.0, 40.0);
            p.fill_rect(b2, 16.0, well_h);
            p.text(
                "Is the sea warm enough to swim then?",
                Point::new(b2.x0 + 14.0, b2.y0 + 11.0),
                &TextStyle::new(13.5).color(ink),
            );
            p.text(
                "Still cool for long swims. Most families paddle and play at the edge.",
                Point::new(cx0, win.y0 + 336.0),
                &TextStyle::new(13.5).color(ink),
            );

            let comp = Rect::from_xywh(cx0, win.y1 - 92.0, col_w, 72.0);
            p.fill_rect(comp, 16.0, well);
            p.text(
                "Reply",
                Point::new(comp.x0 + 16.0, comp.y0 + 12.0),
                &TextStyle::new(14.0).color(muted),
            );
            p.fill_rect(
                Rect::from_xywh(comp.x0 + 12.0, comp.y1 - 34.0, 86.0, 26.0),
                999.0,
                well_h,
            );
            p.text(
                "Think · Auto",
                Point::new(comp.x0 + 22.0, comp.y1 - 28.0),
                &TextStyle::new(12.0).weight(FontWeight::SEMIBOLD).color(ink),
            );
            p.text(
                "18% of context",
                Point::new(comp.x1 - 140.0, comp.y1 - 28.0),
                &TextStyle::new(12.0).color(muted),
            );
            p.fill_circle(Point::new(comp.x1 - 24.0, comp.y1 - 21.0), 15.0, ink);
            p.text(
                "↑",
                Point::new(comp.x1 - 28.5, comp.y1 - 30.0),
                &TextStyle::new(14.0).weight(FontWeight::BOLD).color(fill),
            );

            // Cursor moves to Settings gear and clicks at lt = 3.15s
            let u = smootherstep(1.6, 2.9, lt);
            let cur = Point::new(
                lerp(win.center().x, gear_c.x, u),
                lerp(win.center().y, gear_c.y, u),
            );
            let click_age = (3.05..3.25).contains(&lt).then_some(lt - 3.05);
            draw_cursor(p, cur, click_age, tide, smoothstep(1.4, 1.7, lt) as f32);
        } else {
            // ===== PIVON SETTINGS · ADVANCED (DARK -> WARM STONE LIGHT) =====
            let sb_w = 220.0;
            p.stroke_line(
                Point::new(win.x0 + sb_w, win.y0),
                Point::new(win.x0 + sb_w, win.y1),
                1.0,
                rim,
            );
            let secs = [
                "General",
                "Model and engine",
                "Search and sources",
                "Workspace",
                "Memory",
                "Data and backup",
                "Advanced",
            ];
            for (i, sec) in secs.iter().enumerate() {
                let rr = Rect::from_xywh(
                    win.x0 + 10.0,
                    win.y0 + 50.0 + i as f64 * 34.0,
                    sb_w - 20.0,
                    32.0,
                );
                if i == 6 {
                    p.fill_rect(rr, 10.0, well_h);
                }
                p.text(
                    sec,
                    Point::new(rr.x0 + 10.0, rr.y0 + 7.5),
                    &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(ink),
                );
            }

            let mx = win.x0 + sb_w + 28.0;
            let mw = win.width() - sb_w - 56.0;
            p.text(
                "Advanced",
                Point::new(mx, win.y0 + 18.0),
                &TextStyle::new(20.0).weight(FontWeight::SEMIBOLD).color(ink),
            );

            let g1_y = win.y0 + 62.0;
            p.text(
                "Context and memory",
                Point::new(mx + 14.0, g1_y),
                &TextStyle::new(12.0)
                    .weight(FontWeight::SEMIBOLD)
                    .color(muted),
            );
            let w1 = Rect::from_xywh(mx, g1_y + 22.0, mw, 132.0);
            p.fill_rect(w1, 16.0, well);
            let rows = [
                ("Context length", "32K ▾"),
                ("Model memory budget", "Automatic · 8 GB ▾"),
                ("KV cache precision", "8-bit ▾"),
            ];
            let mut q_center = Point::ZERO;
            for (i, (lbl, val)) in rows.into_iter().enumerate() {
                let ry = w1.y0 + i as f64 * 44.0;
                if i > 0 {
                    p.stroke_line(Point::new(w1.x0, ry), Point::new(w1.x1, ry), 1.0, rim);
                }
                p.text(
                    lbl,
                    Point::new(w1.x0 + 14.0, ry + 13.0),
                    &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(ink),
                );
                let lw = p
                    .measure_text(lbl, None, &TextStyle::new(13.0).weight(FontWeight::MEDIUM))
                    .width;
                let qc = Point::new(w1.x0 + 26.0 + lw, ry + 22.0);
                if i == 0 {
                    q_center = qc;
                }
                let q_active = i == 0 && lt > 4.6;
                p.fill_circle(qc, 8.0, if q_active { ink } else { well_h });
                p.text(
                    "?",
                    Point::new(qc.x - 3.0, qc.y - 7.0),
                    &TextStyle::new(11.0)
                        .weight(FontWeight::BOLD)
                        .color(if q_active { fill } else { muted }),
                );
                let vw = p
                    .measure_text(val, None, &TextStyle::new(13.0).weight(FontWeight::MEDIUM))
                    .width
                    + 20.0;
                p.fill_rect(
                    Rect::from_xywh(w1.x1 - vw - 14.0, ry + 8.0, vw, 28.0),
                    8.0,
                    well_h,
                );
                p.text(
                    val,
                    Point::new(w1.x1 - vw - 4.0, ry + 13.5),
                    &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(ink),
                );
            }

            // Thinking group
            let g2_y = w1.y1 + 22.0;
            p.text(
                "Thinking",
                Point::new(mx + 14.0, g2_y),
                &TextStyle::new(12.0)
                    .weight(FontWeight::SEMIBOLD)
                    .color(muted),
            );
            let w2 = Rect::from_xywh(mx, g2_y + 22.0, mw, 88.0);
            p.fill_rect(w2, 16.0, well);
            p.text(
                "Tokens per level",
                Point::new(w2.x0 + 14.0, w2.y0 + 13.0),
                &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(ink),
            );
            for (i, (k, v)) in [("Quick", "512"), ("Balanced", "1536"), ("Deep", "4096")]
                .into_iter()
                .enumerate()
            {
                let bx = w2.x1 - 344.0 + i as f64 * 112.0;
                p.text(
                    k,
                    Point::new(bx, w2.y0 + 14.0),
                    &TextStyle::new(12.0).color(muted),
                );
                p.fill_rect(
                    Rect::from_xywh(bx + 56.0, w2.y0 + 8.0, 50.0, 28.0),
                    8.0,
                    well_h,
                );
                p.text(
                    v,
                    Point::new(bx + 64.0, w2.y0 + 13.5),
                    &TextStyle::new(12.5).color(ink),
                );
            }
            p.stroke_line(
                Point::new(w2.x0, w2.y0 + 44.0),
                Point::new(w2.x1, w2.y0 + 44.0),
                1.0,
                rim,
            );
            p.text(
                "Offer Auto thinking",
                Point::new(w2.x0 + 14.0, w2.y0 + 57.0),
                &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(ink),
            );
            let tog_u = smootherstep(5.6, 5.9, lt);
            p.fill_rect(
                Rect::from_xywh(w2.x1 - 50.0, w2.y0 + 55.0, 36.0, 22.0),
                999.0,
                ink.mix(well_h, tog_u as f32),
            );
            p.fill_circle(
                Point::new(w2.x1 - lerp(25.0, 39.0, tog_u), w2.y0 + 66.0),
                8.0,
                fill.mix(muted, tog_u as f32),
            );

            // Tools group
            let g3_y = w2.y1 + 22.0;
            p.text(
                "Tools",
                Point::new(mx + 14.0, g3_y),
                &TextStyle::new(12.0)
                    .weight(FontWeight::SEMIBOLD)
                    .color(muted),
            );
            let w3 = Rect::from_xywh(mx, g3_y + 22.0, mw, 44.0);
            p.fill_rect(w3, 16.0, well);
            p.text(
                "Offer only relevant tools",
                Point::new(w3.x0 + 14.0, w3.y0 + 13.0),
                &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(ink),
            );
            p.fill_rect(
                Rect::from_xywh(w3.x1 - 50.0, w3.y0 + 11.0, 36.0, 22.0),
                999.0,
                ink,
            );
            p.fill_circle(Point::new(w3.x1 - 25.0, w3.y0 + 22.0), 8.0, fill);

            // Solid-ink "?" Hint Tooltip pops in smoothly when cursor hovers "?"
            if lt > 4.6 {
                let ta = smoothstep(4.6, 4.9, lt) as f32;
                let tip = Rect::from_xywh(w1.x0 + 134.0, w1.y0 - 58.0, 276.0, 66.0);
                p.shadow(
                    tip,
                    14.0,
                    &Shadow::new(
                        Color::rgba(0.0, 0.0, 0.0, 0.24 * ta),
                        Vec2::new(0.0, 8.0),
                        22.0,
                    ),
                );
                p.fill_rect(tip, 14.0, ink.with_alpha(ta));
                p.text(
                    "How much of the chat the model reads",
                    Point::new(tip.x0 + 12.0, tip.y0 + 11.0),
                    &TextStyle::new(12.0)
                        .weight(FontWeight::MEDIUM)
                        .color(fill.with_alpha(ta)),
                );
                p.text(
                    "at once. Past 48K, it exceeds 8 GB.",
                    Point::new(tip.x0 + 12.0, tip.y0 + 29.0),
                    &TextStyle::new(12.0)
                        .weight(FontWeight::MEDIUM)
                        .color(fill.with_alpha(ta)),
                );
            }

            // Cursor hovers "?" then moves to toggle
            let tog_pt = Point::new(w2.x1 - 32.0, w2.y0 + 66.0);
            let cur = if lt < 5.2 {
                let u = smootherstep(3.8, 4.55, lt);
                Point::new(
                    lerp(win.x0 + 200.0, q_center.x, u),
                    lerp(win.y1 - 80.0, q_center.y, u),
                )
            } else {
                let u = smootherstep(5.2, 5.58, lt);
                Point::new(lerp(q_center.x, tog_pt.x, u), lerp(q_center.y, tog_pt.y, u))
            };
            let click_age = (5.60..6.02).contains(&lt).then_some(lt - 5.60);
            draw_cursor(p, cur, click_age, tide, 1.0);
        }
    });

    draw_caption(
        p,
        if is_settings {
            "SETTINGS · HINTS ONLY WHERE THEY MATTER"
        } else {
            "PIVON WINDOW · HISTORY"
        },
        if is_settings {
            "Warm Stone Light finish · Interactive ? tooltips & toggles"
        } else {
            "Full history sidebar, search, and live model status"
        },
        1.0,
        light_mix,
    );
    light_mix
}

// ---- ACT 5 (28.5s .. 34.0s): THREE FINISHES (DARK · LIGHT · AMOLED) & ISLAND STATES -----

fn draw_act_finishes(p: &mut Painter<'_>, lt: f64, global_t: f64) {
    let fade = (smoothstep(0.0, 0.4, lt) * (1.0 - smoothstep(4.9, 5.5, lt))) as f32;
    let tide = Color::from_hex(TIDE_DARK);

    draw_centered_text(
        p,
        "Three finishes. Every edge.",
        LOGICAL_W * 0.5,
        66.0,
        &TextStyle::new(34.0)
            .weight(FontWeight::SEMIBOLD)
            .letter_spacing(-1.0)
            .color(Color::from_hex(0xF4F4F1).with_alpha(fade)),
    );

    let finishes = [
        (
            "Dark",
            DESK_DARK_A,
            DESK_DARK_B,
            FILL_DARK,
            INK_DARK,
            MUTED_DARK,
            TIDE_DARK,
            "Reading notes.md",
            "3 of 8",
        ),
        (
            "Light",
            DESK_LIGHT_A,
            DESK_LIGHT_B,
            FILL_LIGHT,
            INK_LIGHT,
            MUTED_LIGHT,
            TIDE_LIGHT,
            "Searching the web",
            "2 of 4",
        ),
        (
            "AMOLED",
            0x0A0A0A,
            0x000000,
            0x000000,
            0xE6E6E6,
            0x8F8F8F,
            TIDE_DARK,
            "Answer ready",
            "Burgas trip",
        ),
    ];

    for (i, (name, da, db, fill_h, ink_h, mut_h, ag_h, lbl, val)) in
        finishes.into_iter().enumerate()
    {
        let enter = ease_out_back((lt - i as f64 * 0.14) / 0.55);
        let cw = 368.0;
        let ch = 410.0;
        let cx = 64.0 + i as f64 * (cw + 24.0);
        let cy = 148.0 + (1.0 - enter) * 28.0;
        let card = Rect::from_xywh(cx, cy, cw, ch);

        p.shadow(
            card,
            22.0,
            &Shadow::new(
                Color::rgba(0.0, 0.0, 0.0, 0.42 * fade),
                Vec2::new(0.0, 18.0),
                38.0,
            ),
        );
        p.fill_rect(
            card,
            22.0,
            Brush::linear(
                Point::new(card.x0, card.y0),
                Point::new(card.x1, card.y1),
                [
                    (0.0, Color::from_hex(da).with_alpha(fade)),
                    (1.0, Color::from_hex(db).with_alpha(fade)),
                ],
            ),
        );
        p.stroke_rect(card, 22.0, 1.0, Color::rgba(1.0, 1.0, 1.0, 0.10 * fade));

        let fill = Color::from_hex(fill_h).with_alpha(fade);
        let ink = Color::from_hex(ink_h).with_alpha(fade);
        let muted = Color::from_hex(mut_h).with_alpha(fade);
        let agent = Color::from_hex(ag_h).with_alpha(fade);

        // Floating pill at top of each finish card (No-notch display mode)
        let pill = Rect::from_xywh(card.center().x - 122.0, card.y0 + 20.0, 244.0, 38.0);
        p.shadow(
            pill,
            999.0,
            &Shadow::new(
                Color::rgba(0.0, 0.0, 0.0, 0.25 * fade),
                Vec2::new(0.0, 8.0),
                18.0,
            ),
        );
        p.fill_rect(pill, 999.0, fill);
        if i == 0 {
            draw_pip_orbit(
                p,
                Point::new(pill.x0 + 20.0, pill.center().y),
                5.0,
                agent,
                global_t,
            );
        } else if i == 1 {
            draw_pip_ripple(
                p,
                Point::new(pill.x0 + 20.0, pill.center().y),
                4.0,
                agent,
                global_t,
            );
        } else {
            p.text(
                "✓",
                Point::new(pill.x0 + 14.0, pill.y0 + 10.0),
                &TextStyle::new(13.0).weight(FontWeight::BOLD).color(agent),
            );
        }
        p.text(
            lbl,
            Point::new(pill.x0 + 34.0, pill.y0 + 11.0),
            &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(ink),
        );
        p.text(
            val,
            Point::new(pill.x1 - 74.0, pill.y0 + 11.0),
            &TextStyle::new(12.5)
                .weight(FontWeight::SEMIBOLD)
                .color(muted),
        );

        // Mini Bead card preview inside each finish
        let inner = Rect::from_xywh(card.x0 + 24.0, card.y0 + 82.0, card.width() - 48.0, 242.0);
        p.shadow(
            inner,
            24.0,
            &Shadow::new(
                Color::rgba(0.0, 0.0, 0.0, 0.28 * fade),
                Vec2::new(0.0, 14.0),
                28.0,
            ),
        );
        p.fill_rect(inner, 24.0, fill);
        let well_c = if i == 0 {
            Color::from_hex(WELL_DARK)
        } else if i == 1 {
            Color::from_hex(WELL_LIGHT)
        } else {
            Color::from_hex(0x141414)
        }
        .with_alpha(fade);
        p.text(
            "Pivon on Bead",
            Point::new(inner.x0 + 18.0, inner.y0 + 18.0),
            &TextStyle::new(15.0).weight(FontWeight::SEMIBOLD).color(ink),
        );
        p.fill_rect(
            Rect::from_xywh(inner.x0 + 14.0, inner.y0 + 52.0, inner.width() - 28.0, 92.0),
            14.0,
            well_c,
        );
        p.text(
            "Ask anything",
            Point::new(inner.x0 + 28.0, inner.y0 + 66.0),
            &TextStyle::new(13.5).color(muted),
        );
        p.fill_circle(Point::new(inner.x1 - 36.0, inner.y0 + 124.0), 13.0, ink);
        p.text(
            "↑",
            Point::new(inner.x1 - 40.0, inner.y0 + 115.5),
            &TextStyle::new(13.0).weight(FontWeight::BOLD).color(fill),
        );
        p.fill_rect(
            Rect::from_xywh(
                inner.x0 + 14.0,
                inner.y0 + 158.0,
                inner.width() - 28.0,
                32.0,
            ),
            10.0,
            well_c,
        );
        p.text(
            "Weekend trip to Burgas",
            Point::new(inner.x0 + 26.0, inner.y0 + 166.0),
            &TextStyle::new(12.5).weight(FontWeight::MEDIUM).color(ink),
        );
        p.text(
            "Qwen3.5 4B · Ready",
            Point::new(inner.x0 + 26.0, inner.y0 + 206.0),
            &TextStyle::new(12.0).color(muted),
        );
        draw_pip_breathe(
            p,
            Point::new(inner.x1 - 28.0, inner.y0 + 214.0),
            4.0,
            agent,
            global_t,
        );

        draw_centered_text(
            p,
            name,
            card.center().x,
            card.y1 - 48.0,
            &TextStyle::new(18.0)
                .weight(FontWeight::SEMIBOLD)
                .color(if i == 1 {
                    Color::from_hex(0x2A2A27).with_alpha(fade)
                } else {
                    Color::from_hex(0xECECE8).with_alpha(fade)
                }),
        );
    }

    draw_caption(
        p,
        "BEAD DESIGN TOKENS",
        "Dark · Warm Stone Light · True AMOLED — all driven by reactive themes",
        fade,
        0.0,
    );
    let _ = tide;
}

// ---- ACT 6 (34.0s .. 38.0s): QUIET OUTRO — FOLDS BACK INTO THE NOTCH --------------------

fn draw_act_outro(p: &mut Painter<'_>, lt: f64, global_t: f64) {
    let a = (smoothstep(0.0, 0.5, lt) * (1.0 - smoothstep(3.4, 4.0, lt))) as f32;
    let tide = Color::from_hex(TIDE_DARK);

    // The single resting notch at top-center with its breathing Tide dot
    let island = draw_notch_shell(p, LOGICAL_W * 0.5, 0.0, 210.0, 34.0, 16.0);
    draw_pip_breathe(
        p,
        Point::new(island.x0 + 18.0, 17.0),
        4.0,
        tide.with_alpha(a),
        global_t,
    );

    draw_centered_text(
        p,
        "rugkit 0.1",
        LOGICAL_W * 0.5,
        244.0,
        &TextStyle::new(72.0)
            .weight(FontWeight::SEMIBOLD)
            .letter_spacing(-3.0)
            .color(Color::from_hex(0xF4F4F1).with_alpha(a)),
    );
    draw_centered_text(
        p,
        "The one framework that cares about visuals.",
        LOGICAL_W * 0.5,
        336.0,
        &TextStyle::new(22.0).color(Color::from_hex(0xA9A9A4).with_alpha(a)),
    );
    draw_centered_text(
        p,
        "< 8 ms frames  ·  139 ms cold start  ·  7.9 MB binary  ·  Pure Rust",
        LOGICAL_W * 0.5,
        384.0,
        &TextStyle::new(14.0)
            .weight(FontWeight::MEDIUM)
            .color(tide.with_alpha(a)),
    );

    let cta = Rect::from_xywh(LOGICAL_W * 0.5 - 196.0, 442.0, 392.0, 40.0);
    p.fill_rect(cta, 999.0, Color::from_hex(0x0E0E10).with_alpha(a));
    p.stroke_rect(cta, 999.0, 1.0, Color::rgba(1.0, 1.0, 1.0, 0.14 * a));
    draw_centered_text(
        p,
        "github.com/Shadrous/rugkit  ·  MIT OR Apache-2.0",
        LOGICAL_W * 0.5,
        cta.y0 + 11.0,
        &TextStyle::new(14.0)
            .weight(FontWeight::SEMIBOLD)
            .color(Color::from_hex(0xF4F4F1).with_alpha(a)),
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "docs/demo/rugkit_launch.mp4".to_string());
    if let Some(parent) = std::path::Path::new(&out_path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Warm, lush, ear-friendly ambient score:
    // - ONLY smooth warm low-register sine chords (Am9 -> Fmaj7 -> Cmaj9 -> G6 in 110..330 Hz)
    // - Gentle deep heartbeat sub-pulse (55 Hz, zero harsh clicks, ZERO space-laser hi-hats!)
    // - Clean fade-in & fade-out
    let audio_expr = concat!(
        "aevalsrc='",
        "(",
        // Warm low-register chord pad (changes smoothly every 4.5s)
        "0.16*sin(2*PI*(if(lt(mod(t,18),4.5),110,if(lt(mod(t,18),9),87.31,if(lt(mod(t,18),13.5),130.81,98))))*t)",
        "+0.12*sin(2*PI*(if(lt(mod(t,18),4.5),164.81,if(lt(mod(t,18),9),130.81,if(lt(mod(t,18),13.5),196,146.83))))*t)",
        "+0.10*sin(2*PI*(if(lt(mod(t,18),4.5),220,if(lt(mod(t,18),9),174.61,if(lt(mod(t,18),13.5),246.94,196))))*t)",
        "+0.07*sin(2*PI*(if(lt(mod(t,18),4.5),261.63,if(lt(mod(t,18),9),220,if(lt(mod(t,18),13.5),293.66,246.94))))*t)*(0.7+0.3*sin(0.7*t))",
        // Soft muted deep sub-pulse (gentle 55 Hz, no high-frequency transient)
        "+(gte(t,2.5)*lte(t,35.0))*0.16*exp(-mod(t,0.75)*7.5)*sin(2*PI*55*t)",
        ")",
        "*min(1,t/1.8)*min(1,max(0,(38.0-t)/2.4))",
        "':s=48000:d=38.0"
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
            "19",
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

    println!("Rendering {TOTAL_FRAMES} frames ({OUT_W}x{OUT_H} @ {FPS}fps) — Pivon on Bead…");

    for frame in 0..TOTAL_FRAMES {
        let t = f64::from(frame) / f64::from(FPS);
        let light_mix = if (21.5..28.5).contains(&t) {
            smootherstep(3.4, 4.4, t - 21.5) as f32
        } else if (28.5..29.2).contains(&t) {
            (1.0 - smootherstep(28.5, 29.2, t)) as f32
        } else {
            0.0
        };

        {
            let mut p = canvas.begin_frame(SCALE);
            draw_desk(&mut p, light_mix);

            if t < 10.5 {
                draw_act_island_lifecycle(&mut p, t);
            } else if t < 15.0 {
                draw_act_approvals(&mut p, t - 10.5);
            } else if t < 21.5 {
                draw_act_conversation(&mut p, t - 15.0, t);
            } else if t < 28.5 {
                let _ = draw_act_window_and_settings(&mut p, t - 21.5, t);
            } else if t < 34.0 {
                draw_act_finishes(&mut p, t - 28.5, t);
            } else {
                draw_act_outro(&mut p, t - 34.0, t);
            }
        }

        let rgba = offscreen.render_to_rgba(&canvas, OUT_W, OUT_H, Color::from_hex(DESK_DARK_B))?;
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
