//! In-app developer inspector (`⌘⌥I`): layout bounds, widget tree summary, live token
//! editing (light/dark & accent presets), slow-motion animation and frame timing.

use rugkit_render::{Color, FontFamily, FontWeight, Painter, Point, Rect, TextStyle};
use rugkit_shell::{FrameInfo, Key, Modifiers};

use crate::theme::{Theme, set_theme, theme};
use crate::tree::{NodeId, Tree};

const ACCENT_PRESETS: [(&str, u32); 5] = [
    ("Violet", 0x7B61FF),
    ("Coral", 0xFF5A5F),
    ("Mint", 0x00B894),
    ("Sky", 0x2F80ED),
    ("Amber", 0xFF9F1C),
];

/// State for the built-in in-app inspector overlay.
#[derive(Debug)]
pub(crate) struct Inspector {
    pub open: bool,
    pub show_bounds: bool,
    pub time_scale: f64,
    accent_idx: usize,
    dark_override: bool,
}

impl Default for Inspector {
    fn default() -> Self {
        Self {
            open: false,
            show_bounds: true,
            time_scale: 1.0,
            accent_idx: 0,
            dark_override: false,
        }
    }
}

impl Inspector {
    /// Handles inspector hotkeys (`⌘⌥I` to toggle; `B`, `S`, `L`, `A` when open).
    /// Returns `true` if the key event was consumed.
    pub fn handle_key(&mut self, key: &Key, modifiers: Modifiers) -> bool {
        if modifiers.command()
            && modifiers.alt
            && matches!(key, Key::Character(c) if c.eq_ignore_ascii_case("i"))
        {
            self.open = !self.open;
            return true;
        }
        if !self.open || !modifiers.is_empty() {
            return false;
        }
        match key {
            Key::Character(c) if c.eq_ignore_ascii_case("b") => {
                self.show_bounds = !self.show_bounds;
                true
            }
            Key::Character(c) if c.eq_ignore_ascii_case("s") => {
                self.time_scale = if self.time_scale > 0.5 {
                    0.25
                } else if self.time_scale > 0.15 {
                    0.1
                } else {
                    1.0
                };
                true
            }
            Key::Character(c) if c.eq_ignore_ascii_case("l") => {
                self.dark_override = !self.dark_override;
                let base = if self.dark_override {
                    Theme::PLAYFUL_DARK
                } else {
                    Theme::PLAYFUL_LIGHT
                };
                set_theme(base.with_accent(theme().accent));
                true
            }
            Key::Character(c) if c.eq_ignore_ascii_case("a") => {
                self.accent_idx = (self.accent_idx + 1) % ACCENT_PRESETS.len();
                let (_, hex) = ACCENT_PRESETS[self.accent_idx];
                set_theme(theme().with_accent(Color::from_hex(hex)));
                true
            }
            _ => false,
        }
    }

    /// Paints the layout bounds overlay (if enabled) and the floating inspector HUD panel.
    pub fn paint(
        &self,
        p: &mut Painter<'_>,
        tree: &Tree,
        focused: Option<NodeId>,
        frame: &FrameInfo,
    ) {
        if !self.open {
            return;
        }
        if self.show_bounds {
            tree.paint_inspector_bounds(p);
        }

        let w = 276.0;
        let h = 196.0;
        let rect = Rect::from_xywh((frame.size.width - w - 16.0).max(8.0), 16.0, w, h);
        let bg = Color::rgba(0.08, 0.07, 0.14, 0.92);
        let fg = Color::from_hex(0xF5F3FF);
        let muted = Color::from_hex(0xA6A1C4);
        let accent = theme().accent;

        p.fill_rect(rect, 18.0, bg);
        p.stroke_rect(rect, 18.0, 1.5, accent.with_alpha(0.6));

        let title_style = TextStyle::new(14.0).weight(FontWeight::BOLD).color(accent);
        let mono_style = TextStyle::new(12.0).family(FontFamily::Monospace).color(fg);
        let hint_style = TextStyle::new(11.5).family(FontFamily::Ui).color(muted);

        let x = rect.x0 + 14.0;
        let mut y = rect.y0 + 12.0;
        p.text("rugkit inspector  (⌘⌥I)", Point::new(x, y), &title_style);
        y += 24.0;

        let s = frame.stats;
        let gpu = s.gpu_ms.map_or_else(|| "-".into(), |g| format!("{g:.2}ms"));
        let timing = format!("FPS {:>5.1}  CPU {:.2}ms  GPU {gpu}", s.fps, s.cpu_ms);
        p.text(&timing, Point::new(x, y), &mono_style);
        y += 20.0;

        let nodes_line = format!("Nodes: {}   Worst: {:.2}ms", tree.nodes.len(), s.worst_ms);
        p.text(&nodes_line, Point::new(x, y), &mono_style);
        y += 20.0;

        let focus_desc = focused.and_then(|id| tree.nodes.get(id)).map_or_else(
            || "Focus: none".into(),
            |n| {
                let role = n.role.map_or("Node", |r| match r {
                    rugkit_shell::Role::Button => "Button",
                    rugkit_shell::Role::TextInput => "TextInput",
                    rugkit_shell::Role::Checkbox => "Checkbox",
                    rugkit_shell::Role::Switch => "Switch",
                    rugkit_shell::Role::Slider => "Slider",
                    rugkit_shell::Role::RadioButton => "Radio",
                    rugkit_shell::Role::ComboBox => "Select",
                    rugkit_shell::Role::Tab => "Tab",
                    rugkit_shell::Role::List => "List",
                    _ => "Widget",
                });
                let lbl = n.a11y_label.as_deref().unwrap_or("");
                if lbl.is_empty() {
                    format!("Focus: {role}")
                } else {
                    format!("Focus: {role} ({lbl})")
                }
            },
        );
        p.text(&focus_desc, Point::new(x, y), &mono_style);
        y += 22.0;

        let speed_line = format!(
            "Motion: {:.2}x   Bounds: {}",
            self.time_scale,
            if self.show_bounds { "ON" } else { "OFF" }
        );
        p.text(&speed_line, Point::new(x, y), &mono_style);
        y += 22.0;

        let (accent_name, _) = ACCENT_PRESETS[self.accent_idx];
        let theme_line = format!("Accent preset: {accent_name}");
        p.text(&theme_line, Point::new(x, y), &mono_style);
        y += 24.0;

        p.text(
            "[B] bounds  [S] slow-mo  [L] light/dark  [A] accent",
            Point::new(x, y),
            &hint_style,
        );
    }
}
