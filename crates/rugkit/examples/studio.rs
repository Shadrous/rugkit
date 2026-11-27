//! Second Flagship App: **Pulse Studio** — a creative audio-visual synth & motion workstation
//! built with **rugkit**.
//!
//! Showcases:
//! - Real-time 60/120fps reactive `Canvas` 24-band spectrum analyzer & layered Bézier oscilloscope
//!   with `HitRegions` hover frequency readouts
//! - Interactive 4×8 step sequencer with live playhead sweep and per-track colour accents
//! - Synth & FX mixer rack (`Slider`, `Radio`, `Toggle`, `Select`, `TextInput`)
//! - Stems `Table` with custom mini-waveform cell renderers, sorting and multi-row selection
//! - Transparent macOS titlebar, native menu bar, Light/Dark & live accent switching, and `⌘⌥I` inspector
//!
//! ```bash
//! cargo run --release --example studio
//! ```

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::f64::consts::TAU;
use std::rc::Rc;
use std::time::Duration;

use rugkit::prelude::*;
use rugkit::{A11yAction, A11yTree, ImeState, InputEvent, Ui, WakeHandle, WindowCommand};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum WaveShape {
    Sine,
    Saw,
    Square,
}

#[derive(Clone)]
struct StemClip {
    name: &'static str,
    bars: u32,
    gain_db: f64,
    color_hex: u32,
    seed: usize,
}

fn default_stems() -> Vec<StemClip> {
    vec![
        StemClip {
            name: "Neon Kick 808",
            bars: 4,
            gain_db: -1.2,
            color_hex: 0xFF5A5F,
            seed: 11,
        },
        StemClip {
            name: "Crisp Clap & Rim",
            bars: 4,
            gain_db: -3.5,
            color_hex: 0xFF9F1C,
            seed: 23,
        },
        StemClip {
            name: "Velvet Pad Chords",
            bars: 8,
            gain_db: -4.8,
            color_hex: 0x7B61FF,
            seed: 37,
        },
        StemClip {
            name: "Glass Arp Lead",
            bars: 8,
            gain_db: -2.4,
            color_hex: 0x00B894,
            seed: 53,
        },
        StemClip {
            name: "Sub Bass Glide",
            bars: 4,
            gain_db: -0.8,
            color_hex: 0x2F80ED,
            seed: 71,
        },
    ]
}

const TRACK_NAMES: [&str; 4] = ["Kick", "Snare", "Hi-Hat", "Synth"];
const TRACK_COLORS: [u32; 4] = [0xFF5A5F, 0xFF9F1C, 0x00B894, 0x7B61FF];

fn default_pattern() -> [[bool; 8]; 4] {
    [
        [true, false, false, false, true, false, true, false],
        [false, false, true, false, false, false, true, false],
        [true, true, true, true, true, true, true, true],
        [true, false, false, true, false, true, false, true],
    ]
}

#[allow(clippy::too_many_arguments)]
fn studio_view(
    playing: Signal<bool>,
    phase: Signal<f64>,
    step_idx: Signal<usize>,
    bpm: Signal<f64>,
    cutoff: Signal<f64>,
    resonance: Signal<f64>,
    reverb: Signal<f64>,
    wave: Signal<WaveShape>,
    pattern: Signal<[[bool; 8]; 4]>,
    stems: Signal<Vec<StemClip>>,
    project_name: Signal<String>,
    preset: Signal<usize>,
) -> impl View {
    let stereo_wide = signal(true);
    let limiter = signal(true);
    let hovered_band = signal(None::<usize>);
    let selected_stems = signal(BTreeSet::new());

    let band_hits = Rc::new(RefCell::new(HitRegions::<usize>::new()));
    let band_hits_draw = band_hits.clone();
    let band_hits_hover = band_hits;

    // ---- Top Transport Bar ----
    let transport = row((
        row((
            icon(IconName::Sliders).size(22.0).color(|| theme().accent),
            text("Pulse Studio")
                .size(20.0)
                .weight(FontWeight::BLACK)
                .color(|| theme().ink),
            text_input(project_name)
                .placeholder("Session title…")
                .width(190.0),
        ))
        .gap(12.0),
        row((
            button(move || {
                if playing.get() {
                    "Pause".to_string()
                } else {
                    "Play".to_string()
                }
            })
            .padding((9.0, 20.0))
            .on_click(move || playing.update(|p| *p = !*p)),
            text(move || format!("{:.0} BPM", bpm.get()))
                .size(15.0)
                .weight(FontWeight::BOLD)
                .color(|| theme().accent),
            select(
                preset,
                vec![
                    "Midnight Synth".into(),
                    "Solar House".into(),
                    "Ambient Glass".into(),
                ],
            )
            .width(165.0),
            button(move || {
                if theme().background.r < 0.3 {
                    "Light".to_string()
                } else {
                    "Dark".to_string()
                }
            })
            .secondary()
            .padding((9.0, 14.0))
            .on_click(move || {
                let is_dark = theme().background.r < 0.3;
                let base = if is_dark {
                    Theme::PLAYFUL_LIGHT
                } else {
                    Theme::PLAYFUL_DARK
                };
                set_theme(base.with_accent(theme().accent));
            }),
        ))
        .gap(10.0),
    ))
    .fill_width()
    .height(50.0)
    .padding((6.0, 24.0))
    .justify(Justify::SpaceBetween)
    .draggable();

    // ---- Real-Time Spectrum + Oscilloscope Canvas ----
    let visualizer_card = column((
        row((
            text("Live Spectrum & Oscilloscope")
                .size(15.0)
                .weight(FontWeight::BOLD)
                .color(|| theme().ink),
            text(move || {
                hovered_band.get().map_or_else(
                    || "24-band FFT · hover a band".to_string(),
                    |b| format!("Band {} · {} Hz", b + 1, 60 + b * 420),
                )
            })
            .size(12.5)
            .weight(FontWeight::SEMIBOLD)
            .color(|| theme().accent),
        ))
        .fill_width()
        .justify(Justify::SpaceBetween),
        canvas(move |p, rect| {
            let t = theme();
            let ph = phase.get();
            let cut = cutoff.get() / 100.0;
            let res = resonance.get() / 100.0;
            let rev = reverb.get() / 100.0;
            let shape = wave.get();
            let cur_step = step_idx.get();
            let pat = pattern.get();
            let hov = hovered_band.get();

            let mut reg = band_hits_draw.borrow_mut();
            reg.clear();

            // Dark studio screen backdrop inside canvas.
            p.fill_rect(rect, 16.0, Color::from_hex(0x0E0C1D));

            // Subtle grid lines.
            for i in 1..4 {
                let y = rect.y0 + rect.height() * (i as f64 / 4.0);
                p.stroke_line(
                    Point::new(rect.x0 + 12.0, y),
                    Point::new(rect.x1 - 12.0, y),
                    1.0,
                    Color::rgba(1.0, 1.0, 1.0, 0.06),
                );
            }

            // Active step energy boost.
            let kick_hit = if pat[0][cur_step] { 1.0 } else { 0.35 };
            let synth_hit = if pat[3][cur_step] { 1.0 } else { 0.45 };

            // 24 glowing spectrum bars.
            let bands = 24usize;
            let pad_x = 14.0;
            let pad_b = 14.0;
            let usable_w = (rect.width() - pad_x * 2.0).max(1.0);
            let slot_w = usable_w / bands as f64;
            let bar_w = (slot_w * 0.66).max(6.0);
            let max_h = (rect.height() - 28.0).max(10.0);

            for b in 0..bands {
                let bf = b as f64 / bands as f64;
                let filter_env = (1.0 - (bf - cut).max(0.0) * 1.8).clamp(0.18, 1.0);
                let res_peak = (-(bf - cut).powi(2) * 32.0).exp() * res * 0.45;
                let wave_mod = (ph * (1.5 + bf * 3.2) + b as f64 * 0.55).sin().abs();
                let beat_boost = if b < 6 { kick_hit } else { synth_hit };
                let amp = ((0.18 + 0.62 * wave_mod * filter_env + res_peak) * beat_boost)
                    .clamp(0.08, 0.96);

                let bh = max_h * amp;
                let lx = pad_x + b as f64 * slot_w + (slot_w - bar_w) * 0.5;
                let ly = rect.height() - pad_b - bh;
                reg.push_rect(b, Rect::from_xywh(lx, ly, bar_w, bh));

                let sr = Rect::from_xywh(rect.x0 + lx, rect.y0 + ly, bar_w, bh);
                let c_top = t
                    .accent
                    .mix(Color::from_hex(0x00E5A0), bf as f32)
                    .mix(Color::WHITE, if hov == Some(b) { 0.35 } else { 0.1 });
                let c_bot = t.accent.with_alpha(0.35);
                p.fill_rect(
                    sr,
                    5.0,
                    Brush::linear(
                        Point::new(sr.x0, sr.y0),
                        Point::new(sr.x0, sr.y1),
                        [(0.0, c_top), (1.0, c_bot)],
                    ),
                );
                // Peak-hold cap dot.
                p.fill_circle(
                    Point::new(sr.center().x, (sr.y0 - 5.0).max(rect.y0 + 8.0)),
                    2.2,
                    c_top,
                );
            }

            // Layered smooth oscilloscope curve over the spectrum.
            let samples = 64usize;
            let mid_y = rect.y0 + rect.height() * 0.42;
            let amp_y = rect.height() * (0.20 + rev * 0.08);
            let mut osc = Path::new();
            for s in 0..samples {
                let u = s as f64 / (samples - 1) as f64;
                let x = rect.x0 + pad_x + u * usable_w;
                let theta = u * TAU * (2.0 + cut * 3.0) + ph * 2.2;
                let raw = match shape {
                    WaveShape::Sine => theta.sin(),
                    WaveShape::Saw => {
                        let f = (theta / TAU).fract();
                        2.0 * f - 1.0
                    }
                    WaveShape::Square => {
                        if theta.sin() >= 0.0 {
                            0.75
                        } else {
                            -0.75
                        }
                    }
                };
                let env = (u * std::f64::consts::PI).sin();
                let y = mid_y + raw * amp_y * env;
                let pt = Point::new(x, y);
                if s == 0 {
                    osc.move_to(pt);
                } else {
                    osc.line_to(pt);
                }
            }
            p.stroke_path(&osc, 5.5, t.accent.with_alpha(0.28));
            p.stroke_path(&osc, 2.4, Color::WHITE.with_alpha(0.92));
        })
        .width(500.0)
        .height(216.0)
        .on_hover(move |pt| {
            let hit = pt.and_then(|p| band_hits_hover.borrow().hit_test(p).copied());
            hovered_band.set_if_changed(hit);
        }),
    ))
    .gap(10.0)
    .padding(18.0)
    .background(|| theme().surface)
    .border(1.0, || theme().border)
    .radius(24.0)
    .elevation(4.0)
    .shadow_tint(|| theme().accent);

    // ---- 4×8 Step Sequencer ----
    let mut seq_rows: Vec<rugkit::AnyView> = Vec::new();
    for tr in 0..4 {
        let track_color = Color::from_hex(TRACK_COLORS[tr]);
        let mut pads: Vec<rugkit::AnyView> = Vec::new();
        pads.push(
            text(TRACK_NAMES[tr])
                .size(13.0)
                .weight(FontWeight::BOLD)
                .color(track_color)
                .width(54.0)
                .into_any(),
        );
        for st in 0..8 {
            let pad = button("")
                .secondary()
                .width(38.0)
                .height(34.0)
                .padding(0.0)
                .radius(10.0)
                .background(move || {
                    let on = pattern.get()[tr][st];
                    let active_col = step_idx.get() == st;
                    if on {
                        if active_col {
                            track_color.mix(Color::WHITE, 0.25)
                        } else {
                            track_color
                        }
                    } else if active_col {
                        theme().ink.with_alpha(0.16)
                    } else {
                        theme().ink.with_alpha(0.06)
                    }
                })
                .elevation(move || {
                    if pattern.get()[tr][st] && step_idx.get() == st {
                        6.0
                    } else {
                        0.0
                    }
                })
                .scale(move || {
                    if pattern.get()[tr][st] && step_idx.get() == st {
                        1.08
                    } else {
                        1.0
                    }
                })
                .a11y_label(format!("{} step {}", TRACK_NAMES[tr], st + 1))
                .on_click(move || {
                    pattern.update(|p| p[tr][st] = !p[tr][st]);
                })
                .into_any();
            pads.push(pad);
        }
        seq_rows.push(row(pads).gap(6.0).into_any());
    }

    let sequencer_card = column((
        row((
            text("Step Sequencer (8-Step)")
                .size(15.0)
                .weight(FontWeight::BOLD)
                .color(|| theme().ink),
            text(move || format!("Step {} / 8", step_idx.get() + 1))
                .size(12.5)
                .weight(FontWeight::SEMIBOLD)
                .color(|| theme().positive),
        ))
        .fill_width()
        .justify(Justify::SpaceBetween),
        column(seq_rows).gap(10.0).align(Align::Start),
    ))
    .gap(12.0)
    .padding(18.0)
    .background(|| theme().surface)
    .border(1.0, || theme().border)
    .radius(24.0)
    .elevation(4.0)
    .shadow_tint(|| theme().positive);

    // ---- Synth & FX Mixer Rack ----
    let slider_row =
        |label: &'static str, sig: Signal<f64>, min: f64, max: f64, unit: &'static str| {
            row((
                text(label)
                    .size(13.0)
                    .weight(FontWeight::SEMIBOLD)
                    .color(|| theme().ink_soft)
                    .width(82.0),
                slider(sig, min, max)
                    .step(1.0)
                    .width(240.0)
                    .a11y_label(label),
                text(move || format!("{:.0}{unit}", sig.get()))
                    .size(13.0)
                    .weight(FontWeight::BOLD)
                    .color(|| theme().accent)
                    .width(68.0),
            ))
            .gap(10.0)
        };

    let mixer_card = column((
        text("Synth & FX Rack")
            .size(15.0)
            .weight(FontWeight::BOLD)
            .color(|| theme().ink),
        slider_row("Tempo", bpm, 80.0, 160.0, " bpm"),
        slider_row("Cutoff", cutoff, 10.0, 100.0, "%"),
        slider_row("Resonance", resonance, 0.0, 100.0, "%"),
        slider_row("Reverb", reverb, 0.0, 100.0, "%"),
        row((
            radio(wave, WaveShape::Sine).label("Sine"),
            radio(wave, WaveShape::Saw).label("Saw"),
            radio(wave, WaveShape::Square).label("Square"),
        ))
        .gap(16.0),
        row((
            toggle(stereo_wide).label("Stereo Wide"),
            toggle(limiter).label("Master Limiter"),
        ))
        .gap(18.0),
    ))
    .gap(10.0)
    .align(Align::Start)
    .padding(18.0)
    .background(|| theme().surface)
    .border(1.0, || theme().border)
    .radius(24.0)
    .elevation(4.0)
    .shadow_tint(|| theme().accent);

    // ---- Stems Table with Custom Mini-Waveform Cells ----
    let stem_cols = vec![
        Column::text("name", "Stem", 160.0, |s: &StemClip| s.name.to_string()),
        Column::custom("wave", "Waveform", 155.0, |p, s: &StemClip, r, _| {
            let c = Color::from_hex(s.color_hex);
            let bars = 18usize;
            let bw = 4.0;
            let gap = 3.5;
            let x0 = r.x0 + 10.0;
            let cy = r.center().y;
            for i in 0..bars {
                let h = 4.0 + (((i * 17 + s.seed * 7) % 10) as f64) * 1.8;
                p.fill_rect(
                    Rect::from_xywh(x0 + i as f64 * (bw + gap), cy - h * 0.5, bw, h),
                    2.0,
                    c.with_alpha(0.85),
                );
            }
        }),
        Column::text("bars", "Bars", 54.0, |s: &StemClip| format!("{}b", s.bars))
            .align(CellAlign::Center),
        Column::text("gain", "Gain", 76.0, |s: &StemClip| {
            format!("{:+.1} dB", s.gain_db)
        })
        .align(CellAlign::End),
    ];

    let stems_card = column((
        text("Track Stems")
            .size(15.0)
            .weight(FontWeight::BOLD)
            .color(|| theme().ink),
        table(stems, stem_cols)
            .row_height(38.0)
            .header_height(36.0)
            .selection_mode(SelectionMode::Multiple)
            .selected(selected_stems)
            .width(460.0)
            .height(220.0),
    ))
    .gap(10.0)
    .align(Align::Start)
    .padding(18.0)
    .background(|| theme().surface)
    .border(1.0, || theme().border)
    .radius(24.0)
    .elevation(4.0)
    .shadow_tint(|| theme().accent);

    column((
        transport,
        row((visualizer_card, sequencer_card))
            .gap(16.0)
            .padding((0.0, 24.0))
            .align(Align::Start),
        row((mixer_card, stems_card))
            .gap(16.0)
            .padding((0.0, 24.0))
            .align(Align::Start),
    ))
    .gap(14.0)
    .padding((22.0, 0.0))
    .fill_width()
}

struct StudioApp {
    ui: Ui,
    playing: Signal<bool>,
    phase: Signal<f64>,
    step_idx: Signal<usize>,
    bpm: Signal<f64>,
    step_accum: f64,
    exit_after: Option<Duration>,
    forced_theme: Option<Theme>,
}

impl App for StudioApp {
    fn attach(&mut self, wake: WakeHandle) {
        self.ui.attach(wake);
    }

    fn draw(&mut self, painter: &mut Painter<'_>, frame: &FrameInfo) {
        if let Some(t) = self.forced_theme {
            set_theme(t);
        }
        if self.playing.get_untracked() {
            let dt = frame.dt.as_secs_f64().min(1.0 / 20.0);
            let bpm_val = self.bpm.get_untracked();
            let step_dur = 60.0 / (bpm_val * 2.0); // 8th-note steps
            self.step_accum += dt;
            if self.step_accum >= step_dur {
                self.step_accum -= step_dur;
                self.step_idx.update(|s| *s = (*s + 1) % 8);
            }
            self.phase.update(|p| *p += dt * (bpm_val / 60.0) * 2.5);
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
        self.playing.get_untracked() || self.exit_after.is_some() || self.ui.wants_frame()
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
    let forced_theme = match std::env::var("STUDIO_THEME").ok().as_deref() {
        Some("light") => Some(Theme::PLAYFUL_LIGHT),
        Some("dark") => Some(Theme::PLAYFUL_DARK),
        _ => None,
    };
    if let Some(t) = forced_theme {
        set_theme(t);
    }

    let playing = signal(true);
    let phase = signal(0.0f64);
    let step_idx = signal(0usize);
    let bpm = signal(124.0f64);
    let cutoff = signal(72.0f64);
    let resonance = signal(48.0f64);
    let reverb = signal(38.0f64);
    let wave = signal(WaveShape::Sine);
    let pattern = signal(default_pattern());
    let stems = signal(default_stems());
    let project_name = signal(String::from("Midnight Launch"));
    let preset = signal(0usize);

    let ui = Ui::new(move || {
        studio_view(
            playing,
            phase,
            step_idx,
            bpm,
            cutoff,
            resonance,
            reverb,
            wave,
            pattern,
            stems,
            project_name,
            preset,
        )
    })
    .inspector_enabled(true);

    rugkit::run(
        WindowOptions {
            title: "Pulse Studio · rugkit flagship".into(),
            size: Size::new(1080.0, 740.0),
            background: theme().background,
            transparent_titlebar: true,
            menu_bar: Some(MenuBar::standard_app("Pulse Studio")),
            ..WindowOptions::default()
        },
        StudioApp {
            ui,
            playing,
            phase,
            step_idx,
            bpm,
            step_accum: 0.0,
            exit_after: std::env::var("STUDIO_EXIT_AFTER")
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .map(Duration::from_secs_f64),
            forced_theme,
        },
    )
}
