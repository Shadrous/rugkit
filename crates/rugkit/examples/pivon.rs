//! **Pivon on Bead** — flagship recreation of the Pivon notch-island AI assistant in `rugkit`.
//!
//! Showcases:
//! - The **Bead** design language in all 3 finishes: **Dark** (`#0e0e10`), **Light** (`#fafaf8`),
//!   and **AMOLED** (`#000000`) over an architectural matte desk.
//! - The living **Notch Island** that organically springs between **Resting**, **Peek**,
//!   **Summon** (with in-place Thinking/Depth options), **Thinking** (orbiting Tide ring),
//!   **Working** (rippling pip + 2px progress rim), **Approval** (`ls -la reports/`),
//!   **Memory Review**, and the full **Conversation Stage** (streaming text, citations, source pills).
//! - The full **Pivon Window** with **History** sidebar and **Settings** (`?` hint tooltips,
//!   selects, numeric token fields, and toggles).
//!
//! Run:
//! ```bash
//! cargo run --release --example pivon
//! ```

use std::f64::consts::TAU;
use std::time::Instant;

use rugkit::prelude::*;
use rugkit::{HitRegions, IconName};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Finish {
    Dark,
    Light,
    Amoled,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SurfaceMode {
    Island,
    HistoryWindow,
    SettingsWindow,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum IslandState {
    Resting,
    Peek,
    Summon,
    Thinking,
    Working,
    Approval,
    Memory,
    Conversation,
}

#[derive(Clone, Copy)]
struct BeadPalette {
    desk_a: Color,
    desk_b: Color,
    desk_ink: Color,
    desk_muted: Color,
    fill: Color,
    well: Color,
    well_hover: Color,
    rim: Color,
    ink: Color,
    ink_muted: Color,
    agent: Color,
}

impl Finish {
    fn palette(self) -> BeadPalette {
        match self {
            Finish::Dark => BeadPalette {
                desk_a: Color::from_hex(0x3A3F4A),
                desk_b: Color::from_hex(0x23252B),
                desk_ink: Color::from_hex(0xECECE8),
                desk_muted: Color::from_hex(0xA9A9A4),
                fill: Color::from_hex(0x0E0E10),
                well: Color::from_hex(0x1C1C1F),
                well_hover: Color::from_hex(0x27272B),
                rim: Color::rgba(1.0, 1.0, 1.0, 0.09),
                ink: Color::from_hex(0xF4F4F1),
                ink_muted: Color::from_hex(0xA1A19C),
                agent: Color::from_hex(0x4CC3E8),
            },
            Finish::Light => BeadPalette {
                desk_a: Color::from_hex(0xE4E0D6),
                desk_b: Color::from_hex(0xCFCABE),
                desk_ink: Color::from_hex(0x2A2A27),
                desk_muted: Color::from_hex(0x5A5852),
                fill: Color::from_hex(0xFAFAF8),
                well: Color::from_hex(0xEEEEEA),
                well_hover: Color::from_hex(0xE4E4DF),
                rim: Color::rgba(0.0, 0.0, 0.0, 0.07),
                ink: Color::from_hex(0x151514),
                ink_muted: Color::from_hex(0x5C5C57),
                agent: Color::from_hex(0x0E7490),
            },
            Finish::Amoled => BeadPalette {
                desk_a: Color::from_hex(0x0A0A0A),
                desk_b: Color::from_hex(0x000000),
                desk_ink: Color::from_hex(0xD8D8D8),
                desk_muted: Color::from_hex(0x8F8F8F),
                fill: Color::from_hex(0x000000),
                well: Color::from_hex(0x141414),
                well_hover: Color::from_hex(0x1F1F1F),
                rim: Color::rgba(1.0, 1.0, 1.0, 0.06),
                ink: Color::from_hex(0xE6E6E6),
                ink_muted: Color::from_hex(0x8F8F8F),
                agent: Color::from_hex(0x4CC3E8),
            },
        }
    }

    fn apply_theme(self) {
        let p = self.palette();
        let base = if self == Finish::Light {
            Theme::PLAYFUL_LIGHT
        } else {
            Theme::PLAYFUL_DARK
        };
        set_theme(Theme {
            background: p.desk_b,
            surface: p.fill,
            ink: p.ink,
            ink_soft: p.ink_muted,
            accent: p.agent,
            ..base
        });
    }
}

fn draw_pip_breathe(p: &mut Painter<'_>, c: Point, r: f64, color: Color, t: f64) {
    let scale = 1.0 + 0.16 * (t * 1.6).sin().abs();
    p.fill_circle(c, r * scale, color);
}

fn draw_pip_orbit(p: &mut Painter<'_>, c: Point, r: f64, color: Color, t: f64) {
    let mut arc = Path::new();
    let start = t * 6.5;
    let steps = 18;
    for i in 0..=steps {
        let a = start + (i as f64 / steps as f64) * (TAU * 0.72);
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
    let phase = (t * 0.75).fract();
    let ring_r = r * (1.0 + phase * 1.7);
    let ring_a = ((1.0 - phase) as f32) * 0.55;
    p.stroke_rect(
        Rect::from_xywh(c.x - ring_r, c.y - ring_r, ring_r * 2.0, ring_r * 2.0),
        999.0,
        1.5,
        color.with_alpha(ring_a),
    );
    p.fill_circle(c, r, color);
}

fn main() -> Result<(), rugkit::Error> {
    Finish::Dark.apply_theme();

    let opts = WindowOptions {
        title: "Pivon on Bead — rugkit".into(),
        size: Size::new(1240.0, 820.0),
        transparent_titlebar: true,
        ..WindowOptions::default()
    };

    rugkit::launch(opts, || {
        let finish = signal(Finish::Dark);
        let mode = signal(SurfaceMode::Island);
        let island_state = signal(IslandState::Summon);
        let show_options = signal(true);
        let web_on = signal(true);
        let think_mode = signal(1usize); // 0=Off, 1=Auto, 2=On
        let depth_mode = signal(1usize); // 0=Quick, 1=Balanced, 2=Deep
        let selected_chat = signal(0usize);
        let selected_setting = signal(6usize); // 6 = Advanced
        let show_hint = signal(true);
        let auto_think = signal(true);
        let relevant_tools = signal(true);
        let search_text = signal(String::new());
        let reply_text = signal(String::new());
        let tick_s = signal(0.0f64);
        let start = Instant::now();

        // Smooth 60Hz animation driver for the living pip, orbiting ring, and progress bar.
        spawn(async move {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(16));
                tick_s.set(start.elapsed().as_secs_f64());
            }
        });

        let state_bar = row((
            button("Resting").secondary().on_click(move || {
                mode.set(SurfaceMode::Island);
                island_state.set(IslandState::Resting);
            }),
            button("Peek").secondary().on_click(move || {
                mode.set(SurfaceMode::Island);
                island_state.set(IslandState::Peek);
            }),
            button("Summon").secondary().on_click(move || {
                mode.set(SurfaceMode::Island);
                island_state.set(IslandState::Summon);
            }),
            button("Thinking").secondary().on_click(move || {
                mode.set(SurfaceMode::Island);
                island_state.set(IslandState::Thinking);
            }),
            button("Working").secondary().on_click(move || {
                mode.set(SurfaceMode::Island);
                island_state.set(IslandState::Working);
            }),
            button("Approval").secondary().on_click(move || {
                mode.set(SurfaceMode::Island);
                island_state.set(IslandState::Approval);
            }),
            button("Memory").secondary().on_click(move || {
                mode.set(SurfaceMode::Island);
                island_state.set(IslandState::Memory);
            }),
            button("Stage").secondary().on_click(move || {
                mode.set(SurfaceMode::Island);
                island_state.set(IslandState::Conversation);
            }),
        ))
        .gap(6.0)
        .align(Align::Center);

        let top_bar = row((
            row((
                icon(IconName::Layers)
                    .size(18.0)
                    .color(move || finish.get().palette().agent),
                text("Pivon on Bead")
                    .size(15.0)
                    .weight(FontWeight::BOLD)
                    .color(move || finish.get().palette().desk_ink),
            ))
            .gap(8.0)
            .align(Align::Center),
            state_bar,
            row((
                button("History")
                    .secondary()
                    .on_click(move || mode.set(SurfaceMode::HistoryWindow)),
                button("Settings")
                    .secondary()
                    .on_click(move || mode.set(SurfaceMode::SettingsWindow)),
                button(move || {
                    match finish.get() {
                        Finish::Dark => "Dark",
                        Finish::Light => "Light",
                        Finish::Amoled => "AMOLED",
                    }
                    .to_string()
                })
                .on_click(move || {
                    let next = match finish.get() {
                        Finish::Dark => Finish::Light,
                        Finish::Light => Finish::Amoled,
                        Finish::Amoled => Finish::Dark,
                    };
                    finish.set(next);
                    next.apply_theme();
                }),
            ))
            .gap(6.0)
            .align(Align::Center),
        ))
        .justify(Justify::SpaceBetween)
        .align(Align::Center)
        .padding((22.0, 20.0))
        .fill_width()
        .draggable();

        // Main interactive Bead canvas — renders the living Notch Island OR the full Pivon Window.
        let stage_canvas = canvas(move |p, rect| {
            let pal = finish.get().palette();
            let t = tick_s.get();
            let cur_mode = mode.get();
            let st = island_state.get();
            let opts_open = show_options.get();
            let is_web = web_on.get();
            let th_m = think_mode.get();
            let dp_m = depth_mode.get();
            let chat_i = selected_chat.get();
            let set_i = selected_setting.get();
            let hint_on = show_hint.get();
            let at_on = auto_think.get();
            let rt_on = relevant_tools.get();

            // Soft architectural matte desk backdrop.
            p.fill_rect(
                rect,
                18.0,
                Brush::linear(
                    Point::new(rect.x0, rect.y0),
                    Point::new(rect.x1, rect.y1),
                    [(0.0, pal.desk_a), (1.0, pal.desk_b)],
                ),
            );

            match cur_mode {
                SurfaceMode::Island => {
                    // At the notch, the island IS the notch: pure black, zero rim.
                    let notch_fill = Color::BLACK;
                    let well = Color::from_hex(0x1C1C1F);
                    let well_h = Color::from_hex(0x27272B);
                    let ink = Color::from_hex(0xF4F4F1);
                    let muted = Color::from_hex(0xA1A19C);
                    let agent = Color::from_hex(0x4CC3E8);

                    let (iw, ih, rad) = match st {
                        IslandState::Resting => (210.0, 32.0, 16.0),
                        IslandState::Peek => (342.0, 32.0, 16.0),
                        IslandState::Thinking => (358.0, 32.0, 16.0),
                        IslandState::Working => (368.0, 32.0, 16.0),
                        IslandState::Summon => (420.0, if opts_open { 418.0 } else { 336.0 }, 28.0),
                        IslandState::Approval => (384.0, 198.0, 28.0),
                        IslandState::Memory => (384.0, 198.0, 28.0),
                        IslandState::Conversation => (444.0, 560.0, 32.0),
                    };

                    let island = Rect::from_xywh(rect.center().x - iw * 0.5, rect.y0, iw, ih);
                    if ih > 40.0 {
                        p.shadow(
                            island,
                            rad,
                            &Shadow::new(Color::rgba(0.0, 0.0, 0.0, 0.48), Vec2::new(0.0, 20.0), 44.0),
                        );
                    }
                    p.fill_rect(island, rad, notch_fill);
                    // Square off top corners so it merges seamlessly with the top edge (notch).
                    p.fill_rect(Rect::from_xywh(island.x0, island.y0, island.width(), 16.0), 0.0, notch_fill);

                    match st {
                        IslandState::Resting => {
                            draw_pip_breathe(p, Point::new(island.x0 + 18.0, island.y0 + 16.0), 4.0, agent, t);
                        }
                        IslandState::Peek => {
                            draw_pip_breathe(p, Point::new(island.x0 + 18.0, island.y0 + 16.0), 4.0, agent, t);
                            p.text("Ready", Point::new(island.x0 + 30.0, island.y0 + 8.0), &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(ink));
                            p.text("Qwen3.5 4B", Point::new(island.x1 - 88.0, island.y0 + 8.0), &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(muted));
                        }
                        IslandState::Thinking => {
                            draw_pip_orbit(p, Point::new(island.x0 + 19.0, island.y0 + 16.0), 5.0, agent, t);
                            p.text("Thinking", Point::new(island.x0 + 32.0, island.y0 + 8.0), &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(ink));
                            let secs = (t as u64 % 9) + 1;
                            p.text(&format!("{secs} s"), Point::new(island.x1 - 36.0, island.y0 + 8.0), &TextStyle::new(13.0).weight(FontWeight::SEMIBOLD).color(muted));
                        }
                        IslandState::Working => {
                            draw_pip_ripple(p, Point::new(island.x0 + 19.0, island.y0 + 16.0), 4.0, agent, t);
                            p.text("Searching the web", Point::new(island.x0 + 32.0, island.y0 + 8.0), &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(ink));
                            let prog = (t * 0.25).fract();
                            let step = ((prog * 4.0).floor() as usize).min(3) + 1;
                            p.text(&format!("{step} of 4"), Point::new(island.x1 - 52.0, island.y0 + 8.0), &TextStyle::new(13.0).weight(FontWeight::SEMIBOLD).color(muted));
                            // 2px Tide progress rim along bottom edge
                            p.fill_rect(
                                Rect::from_xywh(island.x0 + 10.0, island.y1 - 2.5, (island.width() - 20.0) * prog, 2.5),
                                2.0,
                                agent,
                            );
                        }
                        IslandState::Summon => {
                            // Top wings
                            draw_pip_breathe(p, Point::new(island.x0 + 20.0, island.y0 + 16.0), 4.0, agent, t);
                            p.text("Ready", Point::new(island.x0 + 32.0, island.y0 + 8.0), &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(ink));
                            p.text("Qwen3.5 4B", Point::new(island.x1 - 90.0, island.y0 + 8.0), &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(muted));

                            // Composer well
                            let well_h_px = if opts_open { 176.0 } else { 96.0 };
                            let comp = Rect::from_xywh(island.x0 + 12.0, island.y0 + 40.0, island.width() - 24.0, well_h_px);
                            p.fill_rect(comp, 16.0, well);
                            p.text(
                                "Compare these two contracts and list what changed",
                                Point::new(comp.x0 + 14.0, comp.y0 + 12.0),
                                &TextStyle::new(14.5).color(ink),
                            );

                            if opts_open {
                                let ob = Rect::from_xywh(comp.x0 + 10.0, comp.y0 + 44.0, comp.width() - 20.0, 80.0);
                                p.fill_rect(ob, 12.0, well_h);
                                let draw_seg = |p: &mut Painter<'_>, y: f64, lbl: &str, opts: [&str; 3], sel: usize| {
                                    p.text(lbl, Point::new(ob.x0 + 10.0, y + 6.0), &TextStyle::new(12.0).weight(FontWeight::MEDIUM).color(muted));
                                    let track = Rect::from_xywh(ob.x0 + 74.0, y, ob.width() - 84.0, 28.0);
                                    p.fill_rect(track, 999.0, Color::BLACK);
                                    let sw = (track.width() - 4.0) / 3.0;
                                    for (i, &opt) in opts.iter().enumerate() {
                                        let sr = Rect::from_xywh(track.x0 + 2.0 + i as f64 * sw, track.y0 + 2.0, sw, 24.0);
                                        if i == sel {
                                            p.fill_rect(sr, 999.0, ink);
                                        }
                                        let m = p.measure_text(opt, None, &TextStyle::new(12.0).weight(FontWeight::SEMIBOLD));
                                        p.text(
                                            opt,
                                            Point::new(sr.center().x - m.width * 0.5, sr.y0 + 4.5),
                                            &TextStyle::new(12.0).weight(FontWeight::SEMIBOLD).color(if i == sel { Color::BLACK } else { muted }),
                                        );
                                    }
                                };
                                draw_seg(p, ob.y0 + 8.0, "Thinking", ["Off", "Auto", "On"], th_m);
                                draw_seg(p, ob.y0 + 44.0, "Depth", ["Quick", "Balanced", "Deep"], dp_m);
                            }

                            // Composer bottom chips + solid send button
                            let cy = comp.y1 - 36.0;
                            p.fill_rect(Rect::from_xywh(comp.x0 + 12.0, cy, 62.0, 28.0), 999.0, well_h);
                            p.text("Auto", Point::new(comp.x0 + 28.0, cy + 6.5), &TextStyle::new(12.0).weight(FontWeight::SEMIBOLD).color(ink));
                            p.fill_rect(Rect::from_xywh(comp.x0 + 80.0, cy, 56.0, 28.0), 999.0, if is_web { well_h } else { Color::TRANSPARENT });
                            p.text("Web", Point::new(comp.x0 + 95.0, cy + 6.5), &TextStyle::new(12.0).weight(FontWeight::SEMIBOLD).color(if is_web { ink } else { muted }));
                            p.fill_circle(Point::new(comp.x1 - 24.0, cy + 14.0), 15.0, ink);
                            p.text("↑", Point::new(comp.x1 - 28.5, cy + 5.0), &TextStyle::new(14.0).weight(FontWeight::BOLD).color(Color::BLACK));

                            // Recent list
                            let rec_y = comp.y1 + 14.0;
                            p.text("Recent", Point::new(island.x0 + 18.0, rec_y), &TextStyle::new(12.0).weight(FontWeight::MEDIUM).color(muted));
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
                                p.text(title, Point::new(rr.x0 + 12.0, rr.y0 + 9.5), &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(ink));
                                p.text(age, Point::new(rr.x1 - 64.0, rr.y0 + 10.0), &TextStyle::new(12.0).color(muted));
                            }
                        }
                        IslandState::Approval | IslandState::Memory => {
                            let is_cmd = st == IslandState::Approval;
                            p.fill_circle(Point::new(island.x0 + 20.0, island.y0 + 16.0), 6.0, agent);
                            p.text(
                                if is_cmd { "Needs you" } else { "Remember this?" },
                                Point::new(island.x0 + 34.0, island.y0 + 8.0),
                                &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(ink),
                            );
                            p.text(
                                if is_cmd { "1" } else { "2" },
                                Point::new(island.x1 - 26.0, island.y0 + 8.0),
                                &TextStyle::new(13.0).weight(FontWeight::SEMIBOLD).color(muted),
                            );
                            p.text(
                                if is_cmd { "Run this command in your workspace?" } else { "Remember that you want Celsius?" },
                                Point::new(island.x0 + 20.0, island.y0 + 44.0),
                                &TextStyle::new(15.0).weight(FontWeight::SEMIBOLD).color(ink),
                            );
                            p.text(
                                if is_cmd {
                                    "I want to list the reports folder to find last month's file."
                                } else {
                                    "You said it while planning the trip. I'll keep it for every chat."
                                },
                                Point::new(island.x0 + 20.0, island.y0 + 68.0),
                                &TextStyle::new(12.5).color(muted),
                            );
                            let box_r = Rect::from_xywh(island.x0 + 14.0, island.y0 + 98.0, island.width() - 28.0, 42.0);
                            p.fill_rect(box_r, 14.0, well);
                            p.text(
                                if is_cmd { "ls -la reports/" } else { "“Always give me temperatures in Celsius.”" },
                                Point::new(box_r.x0 + 14.0, box_r.y0 + 12.0),
                                &TextStyle::new(12.5)
                                    .family(if is_cmd { FontFamily::Monospace } else { FontFamily::Ui })
                                    .color(ink),
                            );
                            // Action buttons
                            let btn_y = island.y1 - 44.0;
                            if !is_cmd {
                                p.text("1 more to review", Point::new(island.x0 + 20.0, btn_y + 8.0), &TextStyle::new(12.0).color(muted));
                            }
                            let q_r = Rect::from_xywh(island.x1 - 180.0, btn_y, 78.0, 32.0);
                            p.fill_rect(q_r, 999.0, well);
                            p.text("Not now", Point::new(q_r.x0 + 14.0, q_r.y0 + 7.5), &TextStyle::new(12.5).weight(FontWeight::SEMIBOLD).color(ink));
                            let s_r = Rect::from_xywh(island.x1 - 94.0, btn_y, 80.0, 32.0);
                            p.fill_rect(s_r, 999.0, ink);
                            p.text(
                                if is_cmd { "Run" } else { "Remember" },
                                Point::new(s_r.x0 + if is_cmd { 26.0 } else { 10.0 }, s_r.y0 + 7.5),
                                &TextStyle::new(12.5).weight(FontWeight::SEMIBOLD).color(Color::BLACK),
                            );
                        }
                        IslandState::Conversation => {
                            draw_pip_ripple(p, Point::new(island.x0 + 20.0, island.y0 + 16.0), 4.0, agent, t);
                            p.text("Answering", Point::new(island.x0 + 32.0, island.y0 + 8.0), &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(ink));
                            p.text("18%", Point::new(island.x1 - 42.0, island.y0 + 8.0), &TextStyle::new(13.0).weight(FontWeight::SEMIBOLD).color(muted));
                            p.text("Weekend trip to Burgas", Point::new(island.x0 + 20.0, island.y0 + 42.0), &TextStyle::new(15.0).weight(FontWeight::SEMIBOLD).color(ink));

                            // User bubble
                            let ub1 = Rect::from_xywh(island.x0 + 84.0, island.y0 + 76.0, island.width() - 104.0, 52.0);
                            p.fill_rect(ub1, 16.0, well_h);
                            p.text("Plan a relaxed beach weekend in Burgas", Point::new(ub1.x0 + 14.0, ub1.y0 + 10.0), &TextStyle::new(13.5).color(ink));
                            p.text("with two kids, late May.", Point::new(ub1.x0 + 14.0, ub1.y0 + 28.0), &TextStyle::new(13.5).color(ink));

                            // Thought + web search trace
                            p.fill_circle(Point::new(island.x0 + 24.0, island.y0 + 148.0), 3.0, Color::from_hex(0x64645F));
                            p.text("Thought for 6 s", Point::new(island.x0 + 34.0, island.y0 + 141.0), &TextStyle::new(12.0).color(muted));
                            p.text("✓  Searched the web · 4 sources", Point::new(island.x0 + 20.0, island.y0 + 162.0), &TextStyle::new(12.0).color(agent));

                            // Assistant plan
                            let lines = [
                                "Late May is warm enough for the beach and before",
                                "the summer crowds. A plan that keeps days short:",
                                "• Saturday: Central Beach & Sea Garden [1]",
                                "• Sunday: boat to St. Anastasia Island [2]",
                                "• Book the boat a few days ahead; sailings fill [2]",
                            ];
                            for (i, line) in lines.iter().enumerate() {
                                p.text(line, Point::new(island.x0 + 20.0, island.y0 + 190.0 + i as f64 * 21.0), &TextStyle::new(13.5).color(ink));
                            }
                            // Source pills
                            p.fill_rect(Rect::from_xywh(island.x0 + 20.0, island.y0 + 306.0, 112.0, 26.0), 999.0, well);
                            p.text("1  visitburgas.bg", Point::new(island.x0 + 30.0, island.y0 + 312.0), &TextStyle::new(12.0).color(ink));
                            p.fill_rect(Rect::from_xywh(island.x0 + 140.0, island.y0 + 306.0, 114.0, 26.0), 999.0, well);
                            p.text("2  burgasport.bg", Point::new(island.x0 + 150.0, island.y0 + 312.0), &TextStyle::new(12.0).color(ink));

                            // Follow-up user bubble
                            let ub2 = Rect::from_xywh(island.x0 + 136.0, island.y0 + 350.0, island.width() - 156.0, 38.0);
                            p.fill_rect(ub2, 16.0, well_h);
                            p.text("Is the sea warm enough to swim then?", Point::new(ub2.x0 + 14.0, ub2.y0 + 10.0), &TextStyle::new(13.5).color(ink));

                            // Streaming answer with blinking Tide caret
                            p.text("Still cool for long swims. Most families paddle", Point::new(island.x0 + 20.0, island.y0 + 404.0), &TextStyle::new(13.5).color(ink));
                            let stream = "at the edge; a light wetsuit helps kids stay in longer";
                            p.text(stream, Point::new(island.x0 + 20.0, island.y0 + 424.0), &TextStyle::new(13.5).color(ink));
                            if ((t * 2.5) as u64).is_multiple_of(2) {
                                let w = p.measure_text(stream, None, &TextStyle::new(13.5)).width;
                                p.fill_rect(Rect::from_xywh(island.x0 + 24.0 + w, island.y0 + 426.0, 7.0, 14.0), 2.0, agent);
                            }

                            // Reply composer well
                            let rep = Rect::from_xywh(island.x0 + 12.0, island.y1 - 82.0, island.width() - 24.0, 70.0);
                            p.fill_rect(rep, 16.0, well);
                            p.text("Reply", Point::new(rep.x0 + 14.0, rep.y0 + 10.0), &TextStyle::new(13.5).color(muted));
                            p.fill_rect(Rect::from_xywh(rep.x0 + 12.0, rep.y1 - 34.0, 56.0, 26.0), 999.0, well_h);
                            p.text("Auto", Point::new(rep.x0 + 24.0, rep.y1 - 28.0), &TextStyle::new(12.0).weight(FontWeight::SEMIBOLD).color(ink));
                            p.fill_rect(Rect::from_xywh(rep.x0 + 74.0, rep.y1 - 34.0, 52.0, 26.0), 999.0, well_h);
                            p.text("Web", Point::new(rep.x0 + 86.0, rep.y1 - 28.0), &TextStyle::new(12.0).weight(FontWeight::SEMIBOLD).color(ink));
                            p.fill_circle(Point::new(rep.x1 - 22.0, rep.y1 - 21.0), 14.0, well_h);
                            p.fill_rect(Rect::from_xywh(rep.x1 - 26.0, rep.y1 - 25.0, 8.0, 8.0), 2.0, ink);
                        }
                    }

                    // Bottom caption describing the current island state
                    let caption = match st {
                        IslandState::Resting => "Resting — A breathing Tide dot beside the notch.",
                        IslandState::Peek => "Peek — Hover opens the wings to show the loaded model.",
                        IslandState::Summon => "Summon — Click the notch to compose; options grow in place.",
                        IslandState::Thinking => "Thinking — An orbiting Tide ring and live seconds counter.",
                        IslandState::Working => "Working — Rippling pip, step count, and 2px progress rim.",
                        IslandState::Approval => "Needs you — Command approval opens directly from the notch.",
                        IslandState::Memory => "Memory review — Your exact words, never a paraphrase.",
                        IslandState::Conversation => "Conversation stage — Streams right from the notch.",
                    };
                    let cm = p.measure_text(caption, None, &TextStyle::new(13.0));
                    p.text(
                        caption,
                        Point::new(rect.center().x - cm.width * 0.5, rect.y1 - 32.0),
                        &TextStyle::new(13.0).color(pal.desk_muted),
                    );
                }

                SurfaceMode::HistoryWindow => {
                    let win = rect.inset(24.0);
                    p.shadow(win, 14.0, &Shadow::new(Color::rgba(0.0, 0.0, 0.0, 0.42), Vec2::new(0.0, 18.0), 44.0));
                    p.fill_rect(win, 14.0, pal.fill);
                    p.stroke_rect(win, 14.0, 1.0, pal.rim);

                    let sb_w = 256.0;
                    p.stroke_line(Point::new(win.x0 + sb_w, win.y0), Point::new(win.x0 + sb_w, win.y1), 1.0, pal.rim);

                    // Traffic lights
                    for (i, hex) in [0xFF5F57u32, 0xFEBC2E, 0x28C840].into_iter().enumerate() {
                        p.fill_circle(Point::new(win.x0 + 22.0 + i as f64 * 18.0, win.y0 + 22.0), 5.5, Color::from_hex(hex));
                    }

                    // Search box + "+" button
                    let sr = Rect::from_xywh(win.x0 + 12.0, win.y0 + 44.0, sb_w - 60.0, 32.0);
                    p.fill_rect(sr, 10.0, pal.well);
                    p.text("Search", Point::new(sr.x0 + 12.0, sr.y0 + 8.0), &TextStyle::new(13.0).color(pal.ink_muted));
                    let add_r = Rect::from_xywh(sr.x1 + 6.0, sr.y0, 32.0, 32.0);
                    p.fill_rect(add_r, 999.0, pal.well);
                    p.text("+", Point::new(add_r.x0 + 11.0, add_r.y0 + 6.0), &TextStyle::new(16.0).color(pal.ink));

                    // Conversation history groups
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
                    for (idx, (title, grp)) in items.into_iter().enumerate() {
                        if let Some(g) = grp {
                            p.text(g, Point::new(win.x0 + 20.0, y), &TextStyle::new(12.0).weight(FontWeight::MEDIUM).color(pal.ink_muted));
                            y += 22.0;
                        }
                        let rr = Rect::from_xywh(win.x0 + 12.0, y, sb_w - 24.0, 32.0);
                        if idx == chat_i {
                            p.fill_rect(rr, 10.0, pal.well_hover);
                        }
                        p.text(title, Point::new(rr.x0 + 10.0, rr.y0 + 7.5), &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(pal.ink));
                        y += 34.0;
                    }

                    // Model status footer
                    draw_pip_breathe(p, Point::new(win.x0 + 24.0, win.y1 - 28.0), 4.0, pal.agent, t);
                    p.text("Qwen3.5 4B", Point::new(win.x0 + 38.0, win.y1 - 42.0), &TextStyle::new(13.0).weight(FontWeight::SEMIBOLD).color(pal.ink));
                    p.text("Ready", Point::new(win.x0 + 38.0, win.y1 - 24.0), &TextStyle::new(12.0).color(pal.ink_muted));

                    // Main chat pane
                    let mx = win.x0 + sb_w;
                    let mw = win.width() - sb_w;
                    p.stroke_line(Point::new(mx, win.y0 + 52.0), Point::new(win.x1, win.y0 + 52.0), 1.0, pal.rim);
                    p.text("Weekend trip to Burgas", Point::new(mx + 28.0, win.y0 + 16.0), &TextStyle::new(15.0).weight(FontWeight::SEMIBOLD).color(pal.ink));

                    let col_w = 600.0f64.min(mw - 56.0);
                    let cx0 = mx + (mw - col_w) * 0.5;
                    let b1 = Rect::from_xywh(cx0 + col_w - 390.0, win.y0 + 74.0, 390.0, 42.0);
                    p.fill_rect(b1, 16.0, pal.well_hover);
                    p.text("Plan a relaxed beach weekend in Burgas with two kids, late May.", Point::new(b1.x0 + 14.0, b1.y0 + 12.0), &TextStyle::new(13.0).color(pal.ink));
                    p.text("Thought for 6 s      Searched the web · 4 sources", Point::new(cx0, win.y0 + 132.0), &TextStyle::new(12.0).color(pal.ink_muted));
                    let body = [
                        "Late May is warm enough for the beach and before the summer crowds. A plan that keeps the days short:",
                        "• Saturday: Central Beach in the morning, the Sea Garden playground after lunch.  [1]",
                        "• Sunday: the boat to St. Anastasia Island, a short crossing.  [2]",
                        "• Book the boat a few days ahead; weekend sailings fill up.  [2]",
                    ];
                    for (i, ln) in body.iter().enumerate() {
                        p.text(ln, Point::new(cx0, win.y0 + 162.0 + i as f64 * 24.0), &TextStyle::new(13.5).color(pal.ink));
                    }
                    let b2 = Rect::from_xywh(cx0 + col_w - 280.0, win.y0 + 278.0, 280.0, 40.0);
                    p.fill_rect(b2, 16.0, pal.well_hover);
                    p.text("Is the sea warm enough to swim then?", Point::new(b2.x0 + 14.0, b2.y0 + 11.0), &TextStyle::new(13.5).color(pal.ink));
                    p.text("Still cool for long swims. Most families paddle and play at the edge; a light wetsuit helps the kids stay in longer.", Point::new(cx0, win.y0 + 336.0), &TextStyle::new(13.5).color(pal.ink));

                    // Bottom composer well
                    let comp = Rect::from_xywh(cx0, win.y1 - 92.0, col_w, 72.0);
                    p.fill_rect(comp, 16.0, pal.well);
                    p.text("Reply", Point::new(comp.x0 + 16.0, comp.y0 + 12.0), &TextStyle::new(14.0).color(pal.ink_muted));
                    p.fill_rect(Rect::from_xywh(comp.x0 + 12.0, comp.y1 - 34.0, 86.0, 26.0), 999.0, pal.well_hover);
                    p.text("Think · Auto", Point::new(comp.x0 + 22.0, comp.y1 - 28.0), &TextStyle::new(12.0).weight(FontWeight::SEMIBOLD).color(pal.ink));
                    p.text("Web", Point::new(comp.x0 + 112.0, comp.y1 - 28.0), &TextStyle::new(12.0).weight(FontWeight::SEMIBOLD).color(pal.ink_muted));
                    p.text("18% of context", Point::new(comp.x1 - 140.0, comp.y1 - 28.0), &TextStyle::new(12.0).color(pal.ink_muted));
                    p.fill_circle(Point::new(comp.x1 - 24.0, comp.y1 - 21.0), 15.0, pal.ink);
                    p.text("↑", Point::new(comp.x1 - 28.5, comp.y1 - 30.0), &TextStyle::new(14.0).weight(FontWeight::BOLD).color(pal.fill));
                }

                SurfaceMode::SettingsWindow => {
                    let win = Rect::from_xywh(rect.center().x - 450.0, rect.y0 + 22.0, 900.0, rect.height() - 44.0);
                    p.shadow(win, 14.0, &Shadow::new(Color::rgba(0.0, 0.0, 0.0, 0.36), Vec2::new(0.0, 18.0), 44.0));
                    p.fill_rect(win, 14.0, pal.fill);
                    p.stroke_rect(win, 14.0, 1.0, pal.rim);

                    let sb_w = 220.0;
                    p.stroke_line(Point::new(win.x0 + sb_w, win.y0), Point::new(win.x0 + sb_w, win.y1), 1.0, pal.rim);
                    for (i, hex) in [0xFF5F57u32, 0xFEBC2E, 0x28C840].into_iter().enumerate() {
                        p.fill_circle(Point::new(win.x0 + 22.0 + i as f64 * 18.0, win.y0 + 22.0), 5.5, Color::from_hex(hex));
                    }

                    let sections = [
                        "General",
                        "Model and engine",
                        "Search and sources",
                        "Workspace",
                        "Memory",
                        "Data and backup",
                        "Advanced",
                    ];
                    for (i, sec) in sections.iter().enumerate() {
                        let rr = Rect::from_xywh(win.x0 + 10.0, win.y0 + 50.0 + i as f64 * 34.0, sb_w - 20.0, 32.0);
                        if i == set_i {
                            p.fill_rect(rr, 10.0, pal.well_hover);
                        }
                        p.text(sec, Point::new(rr.x0 + 10.0, rr.y0 + 7.5), &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(pal.ink));
                    }

                    let mx = win.x0 + sb_w + 28.0;
                    let mw = win.width() - sb_w - 56.0;
                    p.text("Advanced", Point::new(mx, win.y0 + 18.0), &TextStyle::new(20.0).weight(FontWeight::SEMIBOLD).color(pal.ink));

                    // Group 1: Context and memory
                    let g1_y = win.y0 + 58.0;
                    p.text("Context and memory", Point::new(mx + 14.0, g1_y), &TextStyle::new(12.0).weight(FontWeight::SEMIBOLD).color(pal.ink_muted));
                    let w1 = Rect::from_xywh(mx, g1_y + 22.0, mw, 132.0);
                    p.fill_rect(w1, 16.0, pal.well);
                    let g1_rows = [
                        ("Context length", "32K ▾"),
                        ("Model memory budget", "Automatic · 8 GB ▾"),
                        ("KV cache precision", "8-bit ▾"),
                    ];
                    for (i, (lbl, val)) in g1_rows.into_iter().enumerate() {
                        let ry = w1.y0 + i as f64 * 44.0;
                        if i > 0 {
                            p.stroke_line(Point::new(w1.x0, ry), Point::new(w1.x1, ry), 1.0, pal.rim);
                        }
                        p.text(lbl, Point::new(w1.x0 + 14.0, ry + 13.0), &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(pal.ink));
                        let lw = p.measure_text(lbl, None, &TextStyle::new(13.0).weight(FontWeight::MEDIUM)).width;
                        p.fill_circle(Point::new(w1.x0 + 26.0 + lw, ry + 22.0), 8.0, pal.well_hover);
                        p.text("?", Point::new(w1.x0 + 23.0 + lw, ry + 15.0), &TextStyle::new(11.0).weight(FontWeight::BOLD).color(pal.ink_muted));
                        let vw = p.measure_text(val, None, &TextStyle::new(13.0).weight(FontWeight::MEDIUM)).width + 20.0;
                        p.fill_rect(Rect::from_xywh(w1.x1 - vw - 14.0, ry + 8.0, vw, 28.0), 8.0, pal.well_hover);
                        p.text(val, Point::new(w1.x1 - vw - 4.0, ry + 13.5), &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(pal.ink));
                    }

                    // Group 2: Thinking
                    let g2_y = w1.y1 + 20.0;
                    p.text("Thinking", Point::new(mx + 14.0, g2_y), &TextStyle::new(12.0).weight(FontWeight::SEMIBOLD).color(pal.ink_muted));
                    let w2 = Rect::from_xywh(mx, g2_y + 22.0, mw, 88.0);
                    p.fill_rect(w2, 16.0, pal.well);
                    p.text("Tokens per level", Point::new(w2.x0 + 14.0, w2.y0 + 13.0), &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(pal.ink));
                    for (i, (k, v)) in [("Quick", "512"), ("Balanced", "1536"), ("Deep", "4096")].into_iter().enumerate() {
                        let bx = w2.x1 - 344.0 + i as f64 * 112.0;
                        p.text(k, Point::new(bx, w2.y0 + 14.0), &TextStyle::new(12.0).color(pal.ink_muted));
                        p.fill_rect(Rect::from_xywh(bx + 56.0, w2.y0 + 8.0, 50.0, 28.0), 8.0, pal.well_hover);
                        p.text(v, Point::new(bx + 64.0, w2.y0 + 13.5), &TextStyle::new(12.5).color(pal.ink));
                    }
                    p.stroke_line(Point::new(w2.x0, w2.y0 + 44.0), Point::new(w2.x1, w2.y0 + 44.0), 1.0, pal.rim);
                    p.text("Offer Auto thinking", Point::new(w2.x0 + 14.0, w2.y0 + 57.0), &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(pal.ink));
                    p.fill_rect(Rect::from_xywh(w2.x1 - 50.0, w2.y0 + 55.0, 36.0, 22.0), 999.0, if at_on { pal.ink } else { pal.well_hover });
                    p.fill_circle(Point::new(w2.x1 - if at_on { 25.0 } else { 39.0 }, w2.y0 + 66.0), 8.0, if at_on { pal.fill } else { pal.ink_muted });

                    // Group 3: Tools
                    let g3_y = w2.y1 + 20.0;
                    p.text("Tools", Point::new(mx + 14.0, g3_y), &TextStyle::new(12.0).weight(FontWeight::SEMIBOLD).color(pal.ink_muted));
                    let w3 = Rect::from_xywh(mx, g3_y + 22.0, mw, 44.0);
                    p.fill_rect(w3, 16.0, pal.well);
                    p.text("Offer only relevant tools", Point::new(w3.x0 + 14.0, w3.y0 + 13.0), &TextStyle::new(13.0).weight(FontWeight::MEDIUM).color(pal.ink));
                    p.fill_rect(Rect::from_xywh(w3.x1 - 50.0, w3.y0 + 11.0, 36.0, 22.0), 999.0, if rt_on { pal.ink } else { pal.well_hover });
                    p.fill_circle(Point::new(w3.x1 - if rt_on { 25.0 } else { 39.0 }, w3.y0 + 22.0), 8.0, if rt_on { pal.fill } else { pal.ink_muted });

                    // Solid-ink "?" Hint Tooltip
                    if hint_on {
                        let tip = Rect::from_xywh(w1.x0 + 132.0, w1.y0 - 52.0, 272.0, 62.0);
                        p.shadow(tip, 14.0, &Shadow::new(Color::rgba(0.0, 0.0, 0.0, 0.30), Vec2::new(0.0, 8.0), 20.0));
                        p.fill_rect(tip, 14.0, pal.ink);
                        p.text("How much of the chat the model reads at", Point::new(tip.x0 + 12.0, tip.y0 + 10.0), &TextStyle::new(12.0).weight(FontWeight::MEDIUM).color(pal.fill));
                        p.text("once. Past 48K, it exceeds the 8 GB budget.", Point::new(tip.x0 + 12.0, tip.y0 + 28.0), &TextStyle::new(12.0).weight(FontWeight::MEDIUM).color(pal.fill));
                    }
                }
            }

            let _ = HitRegions::<usize>::new();
        })
        .grow(1.0)
        .fill_width()
        .on_click(move |_| {
            // Quick interactive cycling on canvas click for effortless exploration
            match mode.get() {
                SurfaceMode::Island => {
                    if island_state.get() == IslandState::Summon {
                        show_options.update(|b| *b = !*b);
                    }
                }
                SurfaceMode::SettingsWindow => show_hint.update(|b| *b = !*b),
                SurfaceMode::HistoryWindow => selected_chat.update(|c| *c = (*c + 1) % 7),
            }
        });

        let _ = (
            web_on,
            think_mode,
            depth_mode,
            selected_setting,
            auto_think,
            relevant_tools,
            search_text,
            reply_text,
        );

        column((top_bar, stage_canvas))
            .padding((8.0, 16.0))
            .gap(10.0)
            .fill_width()
            .grow(1.0)
            .background(move || finish.get().palette().desk_b)
    })
}
