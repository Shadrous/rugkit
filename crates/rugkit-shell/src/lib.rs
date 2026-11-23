//! Windowing, GPU surface and frame loop for rugkit.
//!
//! [`run`] opens a window and calls your [`App`] every frame with a [`Painter`] and
//! [`FrameInfo`] (size, time and [`FrameStats`]). winit and wgpu do the work underneath,
//! but none of their types appear in this API.
//!
//! M0 scope: one window that redraws continuously. Reactive, on-demand redraws arrive in M1.

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rugkit_render::backend::vello::util::{RenderContext, RenderSurface};
use rugkit_render::backend::vello::{self, wgpu};
use rugkit_render::{Canvas, Color, Painter, Size};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

/// Your application: draws a frame when asked.
pub trait App: 'static {
    /// Draws one frame. Called once per display refresh while the window is visible.
    fn draw(&mut self, painter: &mut Painter<'_>, frame: &FrameInfo);
}

/// Settings for the window [`run`] opens.
#[derive(Clone, Debug)]
pub struct WindowOptions {
    /// Window title.
    pub title: String,
    /// Initial content size in logical pixels.
    pub size: Size,
    /// Colour the window is cleared to before each frame is drawn.
    pub background: Color,
    /// Measure GPU time per frame (fills [`FrameStats::gpu_ms`]).
    ///
    /// This makes the CPU wait for the GPU to finish each frame before presenting, which adds
    /// a little latency. Turn it on for profiling, not in shipped apps.
    pub measure_gpu: bool,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: "rugkit".into(),
            size: Size::new(960.0, 640.0),
            background: Color::WHITE,
            measure_gpu: false,
        }
    }
}

/// Information about the frame being drawn.
#[derive(Copy, Clone, Debug)]
pub struct FrameInfo {
    /// Window content size in logical pixels.
    pub size: Size,
    /// Physical pixels per logical pixel.
    pub scale: f64,
    /// Time since the window first drew.
    pub time: Duration,
    /// Time since the previous frame.
    pub dt: Duration,
    /// Rolling performance numbers.
    pub stats: FrameStats,
}

/// Rolling frame timings, averaged over roughly the last half second.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct FrameStats {
    /// Frames presented per second.
    pub fps: f64,
    /// CPU time per frame in milliseconds: your `draw` call plus encoding and submitting the
    /// GPU work. Waiting for vsync doesn't count.
    pub cpu_ms: f64,
    /// Time from the start of the frame until the GPU finished rendering it, in milliseconds.
    /// `None` unless [`WindowOptions::measure_gpu`] is on.
    pub gpu_ms: Option<f64>,
    /// The worst `gpu_ms` (or `cpu_ms` when GPU timing is off) in the last window.
    pub worst_ms: f64,
}

/// Something went wrong opening the window or setting up the GPU.
#[derive(Debug)]
pub struct Error(String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// Opens a window and runs `app` until the window is closed.
///
/// Must be called on the main thread. Returns when the window closes.
pub fn run(options: WindowOptions, app: impl App) -> Result<(), Error> {
    let event_loop = EventLoop::new().map_err(|e| Error(format!("event loop: {e}")))?;
    let mut shell = Shell {
        options,
        app: Box::new(app),
        canvas: Canvas::new(),
        render_cx: RenderContext::new(),
        gpu: None,
        clock: Clock::default(),
        error: None,
    };
    event_loop
        .run_app(&mut shell)
        .map_err(|e| Error(format!("event loop: {e}")))?;
    match shell.error {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

struct Gpu {
    window: Arc<Window>,
    surface: RenderSurface<'static>,
    renderer: vello::Renderer,
}

struct Shell {
    options: WindowOptions,
    app: Box<dyn App>,
    canvas: Canvas,
    render_cx: RenderContext,
    gpu: Option<Gpu>,
    clock: Clock,
    error: Option<Error>,
}

impl Shell {
    fn init_gpu(&mut self, event_loop: &ActiveEventLoop) -> Result<Gpu, Error> {
        let attrs = Window::default_attributes()
            .with_title(self.options.title.clone())
            .with_inner_size(LogicalSize::new(
                self.options.size.width,
                self.options.size.height,
            ));
        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .map_err(|e| Error(format!("window: {e}")))?,
        );
        let size = window.inner_size();
        let surface = pollster::block_on(self.render_cx.create_surface(
            window.clone(),
            size.width.max(1),
            size.height.max(1),
            wgpu::PresentMode::AutoVsync,
        ))
        .map_err(|e| Error(format!("GPU surface: {e}")))?;
        let device = &self.render_cx.devices[surface.dev_id].device;
        let renderer = vello::Renderer::new(
            device,
            vello::RendererOptions {
                antialiasing_support: vello::AaSupport::area_only(),
                ..Default::default()
            },
        )
        .map_err(|e| Error(format!("renderer: {e}")))?;
        Ok(Gpu {
            window,
            surface,
            renderer,
        })
    }

    fn redraw(&mut self) {
        let Some(gpu) = self.gpu.as_mut() else { return };
        let physical = gpu.window.inner_size();
        if physical.width == 0 || physical.height == 0 {
            return;
        }
        let frame_start = Instant::now();
        let scale = gpu.window.scale_factor();
        let (time, dt) = self.clock.tick(frame_start);
        let info = FrameInfo {
            size: Size::new(
                f64::from(physical.width) / scale,
                f64::from(physical.height) / scale,
            ),
            scale,
            time,
            dt,
            stats: self.clock.stats,
        };

        let mut painter = self.canvas.begin_frame(scale);
        self.app.draw(&mut painter, &info);

        let handle = &self.render_cx.devices[gpu.surface.dev_id];
        let params = vello::RenderParams {
            base_color: vello::peniko::Color::new([
                self.options.background.r,
                self.options.background.g,
                self.options.background.b,
                self.options.background.a,
            ]),
            width: gpu.surface.config.width,
            height: gpu.surface.config.height,
            antialiasing_method: vello::AaConfig::Area,
        };
        if let Err(e) = gpu.renderer.render_to_texture(
            &handle.device,
            &handle.queue,
            self.canvas.vello_scene(),
            &gpu.surface.target_view,
            &params,
        ) {
            log::error!("rugkit: render failed: {e}");
            return;
        }
        let cpu = frame_start.elapsed();
        let gpu_time = if self.options.measure_gpu {
            let _ = handle.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(1)),
            });
            Some(frame_start.elapsed())
        } else {
            None
        };

        let surface_texture = match gpu.surface.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.render_cx.configure_surface(&gpu.surface);
                gpu.window.request_redraw();
                return;
            }
            other => {
                log::warn!("rugkit: skipped a frame: {other:?}");
                gpu.window.request_redraw();
                return;
            }
        };
        let target = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = handle
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rugkit blit"),
            });
        gpu.surface.blitter.copy(
            &handle.device,
            &mut encoder,
            &gpu.surface.target_view,
            &target,
        );
        handle.queue.submit([encoder.finish()]);
        gpu.window.pre_present_notify();
        handle.queue.present(surface_texture);

        self.clock.record(cpu, gpu_time);
        gpu.window.request_redraw();
    }
}

impl ApplicationHandler for Shell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        match self.init_gpu(event_loop) {
            Ok(gpu) => {
                gpu.window.request_redraw();
                self.gpu = Some(gpu);
            }
            Err(err) => {
                self.error = Some(err);
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gpu) = self.gpu.as_mut()
                    && size.width > 0
                    && size.height > 0
                {
                    self.render_cx
                        .resize_surface(&mut gpu.surface, size.width, size.height);
                    gpu.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }
}

/// Frame clock and rolling statistics.
#[derive(Default)]
struct Clock {
    start: Option<Instant>,
    last: Option<Instant>,
    stats: FrameStats,
    window_start: Option<Instant>,
    frames: u32,
    cpu_sum: Duration,
    gpu_sum: Duration,
    worst: Duration,
    gpu_seen: bool,
}

impl Clock {
    const WINDOW: Duration = Duration::from_millis(500);

    fn tick(&mut self, now: Instant) -> (Duration, Duration) {
        let start = *self.start.get_or_insert(now);
        let dt = self.last.map_or(Duration::ZERO, |l| now - l);
        self.last = Some(now);
        (now - start, dt)
    }

    fn record(&mut self, cpu: Duration, gpu: Option<Duration>) {
        let now = Instant::now();
        let window_start = *self.window_start.get_or_insert(now);
        self.frames += 1;
        self.cpu_sum += cpu;
        if let Some(g) = gpu {
            self.gpu_sum += g;
            self.gpu_seen = true;
        }
        self.worst = self.worst.max(gpu.unwrap_or(cpu));
        let elapsed = now - window_start;
        if elapsed >= Self::WINDOW {
            let n = f64::from(self.frames);
            let ms = |d: Duration| d.as_secs_f64() * 1000.0;
            self.stats = FrameStats {
                fps: n / elapsed.as_secs_f64(),
                cpu_ms: ms(self.cpu_sum) / n,
                gpu_ms: self.gpu_seen.then(|| ms(self.gpu_sum) / n),
                worst_ms: ms(self.worst),
            };
            *self = Self {
                start: self.start,
                last: self.last,
                stats: self.stats,
                window_start: Some(now),
                ..Self::default()
            };
        }
    }
}
