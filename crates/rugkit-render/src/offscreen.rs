//! Offscreen GPU rendering and pixel snapshot comparison.
//!
//! Used by rugkit's non-blocking pixel snapshot tests to render widgets offscreen through
//! vello/wgpu and compare RGBA8 output with a small per-pixel tolerance.

use vello::wgpu;

use crate::{Canvas, Color};

/// Summary of comparing two RGBA8 images of equal size.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SnapshotDiff {
    /// Total number of pixels compared.
    pub total_pixels: usize,
    /// Number of pixels where any RGBA channel differed by more than the tolerance.
    pub differing_pixels: usize,
    /// Maximum single-channel difference observed (`0..=255`).
    pub max_channel_diff: u8,
}

impl SnapshotDiff {
    /// Fraction of pixels that differed (`0.0..=1.0`).
    pub fn diff_ratio(&self) -> f64 {
        if self.total_pixels == 0 {
            0.0
        } else {
            self.differing_pixels as f64 / self.total_pixels as f64
        }
    }
}

/// Compares two RGBA8 pixel buffers of the same length with per-channel `tolerance`.
pub fn compare_rgba(expected: &[u8], actual: &[u8], tolerance: u8) -> SnapshotDiff {
    let pixels = expected.len().min(actual.len()) / 4;
    let mut differing = 0;
    let mut max_diff = 0u8;
    for i in 0..pixels {
        let base = i * 4;
        let mut pix_diff = 0u8;
        for c in 0..4 {
            let d = expected[base + c].abs_diff(actual[base + c]);
            if d > pix_diff {
                pix_diff = d;
            }
        }
        if pix_diff > max_diff {
            max_diff = pix_diff;
        }
        if pix_diff > tolerance {
            differing += 1;
        }
    }
    SnapshotDiff {
        total_pixels: pixels,
        differing_pixels: differing,
        max_channel_diff: max_diff,
    }
}

/// Headless GPU renderer for offscreen snapshots.
pub struct OffscreenRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: vello::Renderer,
}

impl std::fmt::Debug for OffscreenRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OffscreenRenderer").finish_non_exhaustive()
    }
}

impl OffscreenRenderer {
    /// Initialises a headless wgpu device and vello renderer.
    pub fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .map_err(|e| format!("no GPU adapter: {e}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("rugkit offscreen"),
            ..Default::default()
        }))
        .map_err(|e| format!("GPU device: {e}"))?;
        let renderer = vello::Renderer::new(
            &device,
            vello::RendererOptions {
                antialiasing_support: vello::AaSupport::area_only(),
                ..Default::default()
            },
        )
        .map_err(|e| format!("vello renderer: {e}"))?;
        Ok(Self {
            device,
            queue,
            renderer,
        })
    }

    /// Renders `canvas` into an offscreen `width`×`height` RGBA8 pixel buffer.
    pub fn render_to_rgba(
        &mut self,
        canvas: &Canvas,
        width: u32,
        height: u32,
        background: Color,
    ) -> Result<Vec<u8>, String> {
        let width = width.max(1);
        let height = height.max(1);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rugkit offscreen target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let params = vello::RenderParams {
            base_color: background.to_peniko(),
            width,
            height,
            antialiasing_method: vello::AaConfig::Area,
        };
        self.renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                canvas.vello_scene(),
                &view,
                &params,
            )
            .map_err(|e| format!("offscreen render: {e}"))?;

        let unpadded_bytes_per_row = width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
        let buf_size = u64::from(padded_bytes_per_row) * u64::from(height);
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rugkit offscreen readback"),
            size: buf_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rugkit offscreen copy"),
            });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);

        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });
        let _ = self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(5)),
        });
        rx.recv()
            .map_err(|e| format!("readback channel: {e}"))?
            .map_err(|e| format!("readback map: {e}"))?;

        let mapped = slice
            .get_mapped_range()
            .map_err(|e| format!("mapped range: {e}"))?;
        let mut out = Vec::with_capacity((unpadded_bytes_per_row * height) as usize);
        for row in 0..height as usize {
            let start = row * padded_bytes_per_row as usize;
            out.extend_from_slice(&mapped[start..start + unpadded_bytes_per_row as usize]);
        }
        drop(mapped);
        readback.unmap();
        Ok(out)
    }
}
