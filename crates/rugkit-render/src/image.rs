//! Bitmap [`ImageData`] for [`crate::Painter::draw_image`].

use std::sync::Arc;

use vello::peniko::{Blob, ImageAlphaType, ImageBrush, ImageData as PenikoImage, ImageFormat};

use crate::Color;

/// An immutable RGBA8 bitmap image. Cheap to clone (pixel buffer is reference-counted).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageData {
    pixels: Arc<Vec<u8>>,
    width: u32,
    height: u32,
}

impl ImageData {
    /// Creates an image from tightly-packed non-premultiplied RGBA8 pixels (`width * height * 4` bytes).
    ///
    /// # Panics
    /// Panics if `pixels.len() != (width as usize) * (height as usize) * 4`.
    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        let expected = (width as usize) * (height as usize) * 4;
        assert_eq!(
            pixels.len(),
            expected,
            "rugkit: ImageData expected {expected} RGBA bytes for {width}x{height}, got {}",
            pixels.len()
        );
        Self {
            pixels: Arc::new(pixels),
            width,
            height,
        }
    }

    /// Creates a solid-colour image of the given dimensions.
    pub fn solid(width: u32, height: u32, color: Color) -> Self {
        let r = (color.r.clamp(0.0, 1.0) * 255.0).round() as u8;
        let g = (color.g.clamp(0.0, 1.0) * 255.0).round() as u8;
        let b = (color.b.clamp(0.0, 1.0) * 255.0).round() as u8;
        let a = (color.a.clamp(0.0, 1.0) * 255.0).round() as u8;
        let mut buf = Vec::with_capacity((width as usize) * (height as usize) * 4);
        for _ in 0..(width as usize * height as usize) {
            buf.extend_from_slice(&[r, g, b, a]);
        }
        Self::from_rgba(width, height, buf)
    }

    /// Image width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Image height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Raw RGBA8 pixel bytes.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub(crate) fn to_peniko_brush(&self) -> ImageBrush {
        let data = PenikoImage {
            data: Blob::new(self.pixels.clone()),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: self.width,
            height: self.height,
        };
        ImageBrush::new(data)
    }
}
