//! The [`Painter`]: rugkit's drawing API.

use vello::Scene;
use vello::kurbo::{Affine, Circle, RoundedRect, Stroke};
use vello::peniko::Fill;

use crate::text::TextSystem;
use crate::{Brush, Point, Rect, Shadow, Size, TextStyle};

/// Records one frame's drawing. Get one from [`Canvas::begin_frame`].
///
/// All coordinates are logical pixels. The painter converts them to physical pixels for the
/// window's scale factor, so drawing code never deals with Retina scaling.
pub struct Painter<'a> {
    scene: &'a mut Scene,
    text: &'a mut TextSystem,
    scale: f64,
}

impl std::fmt::Debug for Painter<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Painter")
            .field("scale", &self.scale)
            .finish_non_exhaustive()
    }
}

impl Painter<'_> {
    /// Physical pixels per logical pixel (2.0 on Retina displays).
    pub fn scale(&self) -> f64 {
        self.scale
    }

    fn transform(&self) -> Affine {
        Affine::scale(self.scale)
    }

    /// Fills a rectangle whose corners are rounded by `radius` (clamped to half the shorter side).
    pub fn fill_rect(&mut self, rect: Rect, radius: f64, brush: impl Into<Brush>) {
        let shape = rounded(rect, radius);
        let brush = brush.into().to_peniko();
        self.scene
            .fill(Fill::NonZero, self.transform(), &brush, None, &shape);
    }

    /// Strokes the outline of a rounded rectangle with a line `width` logical pixels wide,
    /// centred on the edge.
    pub fn stroke_rect(&mut self, rect: Rect, radius: f64, width: f64, brush: impl Into<Brush>) {
        let shape = rounded(rect, radius);
        let brush = brush.into().to_peniko();
        self.scene
            .stroke(&Stroke::new(width), self.transform(), &brush, None, &shape);
    }

    /// Fills a circle.
    pub fn fill_circle(&mut self, center: Point, radius: f64, brush: impl Into<Brush>) {
        let shape = Circle::new(center.to_kurbo(), radius);
        let brush = brush.into().to_peniko();
        self.scene
            .fill(Fill::NonZero, self.transform(), &brush, None, &shape);
    }

    /// Draws the soft shadow that a rounded rectangle at `rect` would cast.
    ///
    /// Draw shadows before the shape that casts them.
    pub fn shadow(&mut self, rect: Rect, radius: f64, shadow: &Shadow) {
        let rect = rect.inset(-shadow.spread).translate(shadow.offset);
        let radius = clamp_radius(rect, radius + shadow.spread);
        // CSS-style blur radius is about two standard deviations of the gaussian.
        let std_dev = (shadow.blur * 0.5).max(0.0);
        self.scene.draw_blurred_rounded_rect(
            self.transform(),
            rect.to_kurbo(),
            shadow.color.to_peniko(),
            radius,
            std_dev,
        );
    }

    /// Draws `text` on one line (explicit `\n` still breaks) with its top-left corner at
    /// `origin`, and returns the size it took up.
    pub fn text(&mut self, text: &str, origin: Point, style: &TextStyle) -> Size {
        self.draw_text(text, origin, None, style)
    }

    /// Draws `text` wrapped to `max_width` with its top-left corner at `origin`, and returns
    /// the size it took up.
    pub fn text_wrapped(
        &mut self,
        text: &str,
        origin: Point,
        max_width: f64,
        style: &TextStyle,
    ) -> Size {
        self.draw_text(text, origin, Some(max_width), style)
    }

    /// Measures `text` without drawing it.
    pub fn measure_text(&mut self, text: &str, max_width: Option<f64>, style: &TextStyle) -> Size {
        let scale = self.scale;
        let layout = self.text.layout(text, style, max_width, scale as f32);
        layout_size(layout, scale)
    }

    fn draw_text(
        &mut self,
        text: &str,
        origin: Point,
        max_width: Option<f64>,
        style: &TextStyle,
    ) -> Size {
        let scale = self.scale;
        let layout = self.text.layout(text, style, max_width, scale as f32);
        // Snap the origin to the physical pixel grid so glyphs stay crisp.
        let transform = Affine::translate(((origin.x * scale).round(), (origin.y * scale).round()));
        let color = style.color.to_peniko();
        for line in layout.lines() {
            for item in line.items() {
                let parley::PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let skew = run
                    .synthesis()
                    .skew()
                    .map(|deg| Affine::skew(f64::from(deg).to_radians().tan(), 0.0));
                // Variable-font axis positions (e.g. weight). Usually a handful of values.
                let coords: Vec<i16> = run
                    .normalized_coords()
                    .iter()
                    .map(|c| c.to_bits())
                    .collect();
                self.scene
                    .draw_glyphs(run.font())
                    .font_size(run.font_size())
                    .transform(transform)
                    .glyph_transform(skew)
                    .normalized_coords(&coords)
                    .brush(color)
                    .draw(
                        Fill::NonZero,
                        glyph_run.positioned_glyphs().map(|g| vello::Glyph {
                            id: g.id,
                            x: g.x,
                            y: g.y,
                        }),
                    );
            }
        }
        layout_size(layout, scale)
    }
}

fn layout_size(layout: &parley::Layout<()>, scale: f64) -> Size {
    Size::new(
        f64::from(layout.width()) / scale,
        f64::from(layout.height()) / scale,
    )
}

fn clamp_radius(rect: Rect, radius: f64) -> f64 {
    radius
        .min(rect.width().abs() * 0.5)
        .min(rect.height().abs() * 0.5)
        .max(0.0)
}

fn rounded(rect: Rect, radius: f64) -> RoundedRect {
    RoundedRect::from_rect(rect.to_kurbo(), clamp_radius(rect, radius))
}

/// Holds what a window needs to record frames: the scene being built and the text system.
///
/// Keep one per window and call [`Canvas::begin_frame`] each frame.
#[derive(Default)]
pub struct Canvas {
    scene: Scene,
    text: TextSystem,
}

impl std::fmt::Debug for Canvas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Canvas")
            .field("text", &self.text)
            .finish_non_exhaustive()
    }
}

impl Canvas {
    /// Creates a canvas and discovers fonts.
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears the previous frame and returns a [`Painter`] for a new one at `scale` physical
    /// pixels per logical pixel.
    pub fn begin_frame(&mut self, scale: f64) -> Painter<'_> {
        self.scene.reset();
        Painter {
            scene: &mut self.scene,
            text: &mut self.text,
            scale,
        }
    }

    /// The recorded scene, for the renderer. Internal integration API; see [`crate::backend`].
    #[doc(hidden)]
    pub fn vello_scene(&self) -> &Scene {
        &self.scene
    }
}
