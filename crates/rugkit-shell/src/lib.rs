//! Windowing, GPU surface, input, accessibility, menus, clipboard and frame loop for rugkit.
//!
//! [`run`] opens a window and calls your [`App`] every frame with a [`Painter`] and
//! [`FrameInfo`] (size, time, [`FrameStats`], [`Appearance`] and Reduce Motion). winit, wgpu,
//! AccessKit, muda and arboard do the work underneath, and none of their types appear in this
//! API.
//!
//! Redraws happen on demand: after input events, when woken through a [`WakeHandle`], and for
//! as long as the app says it wants frames (see [`App::wants_frame`], e.g. while animations
//! run).

pub mod a11y;
mod appearance;
pub mod clipboard;
mod input;
pub mod menu;

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rugkit_render::backend::vello::util::{RenderContext, RenderSurface};
use rugkit_render::backend::vello::{self, wgpu};
use rugkit_render::{Canvas, Color, Painter, Point, Rect, Size};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{ElementState, Ime, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey};
use winit::window::{Theme as WinitTheme, Window, WindowId};

pub use a11y::{A11yAction, A11yId, A11yNode, A11yTree, Role};
pub use appearance::{Appearance, ColorScheme};
pub use input::{ImeEvent, InputEvent, Key, Modifiers, PointerButton};
pub use menu::{MenuBar, MenuEntry, MenuItem, Shortcut, StandardMenuItem, Submenu};

/// A cloneable, thread-safe handle that wakes the window's event loop and schedules a redraw.
///
/// Delivered to [`App::attach`] once the event loop starts. Call [`WakeHandle::wake`] from a
/// timer, background task or reactive effect to redraw an idle window.
#[derive(Clone)]
pub struct WakeHandle {
    proxy: EventLoopProxy<UserEvent>,
}

impl fmt::Debug for WakeHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WakeHandle").finish_non_exhaustive()
    }
}

impl WakeHandle {
    /// Wakes the event loop and requests a redraw. Safe to call from any thread.
    pub fn wake(&self) {
        let _ = self.proxy.send_event(UserEvent::Wake);
    }
}

/// IME state reported by an [`App`] so the OS knows whether to compose text and where to place
/// the candidate popup.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct ImeState {
    /// Whether IME composition is enabled (typically `true` while a text input has focus).
    pub enabled: bool,
    /// Caret / text field rectangle in logical window coordinates.
    pub cursor_rect: Option<Rect>,
}

/// One-shot window commands an [`App`] can emit from [`App::take_commands`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WindowCommand {
    /// Starts an interactive window drag (for custom transparent titlebars).
    DragWindow,
    /// Updates the window title.
    SetTitle(String),
}

/// Your application: handles input, accessibility, commands and draws frames when asked.
pub trait App: 'static {
    /// Draws one frame.
    fn draw(&mut self, painter: &mut Painter<'_>, frame: &FrameInfo);

    /// Called once when the event loop is ready, providing a [`WakeHandle`] to schedule
    /// redraws from outside input events (timers, async tasks, reactive effects).
    fn attach(&mut self, wake: WakeHandle) {
        let _ = wake;
    }

    /// Handles an input event. A redraw is always scheduled after an event.
    fn event(&mut self, event: &InputEvent) {
        let _ = event;
    }

    /// Whether another frame should follow the one just drawn (e.g. an animation is running).
    ///
    /// The default is `true`: redraw continuously. Return `false` when idle so the app uses
    /// no CPU or GPU until the next event.
    fn wants_frame(&self) -> bool {
        true
    }

    /// Reports whether IME composition should be active and where the caret is.
    fn ime_state(&self) -> Option<ImeState> {
        None
    }

    /// Returns the current accessibility tree for screen readers (VoiceOver via AccessKit).
    fn a11y_tree(&mut self) -> Option<A11yTree> {
        None
    }

    /// Handles an accessibility action from a screen reader or test harness.
    fn a11y_action(&mut self, action: &A11yAction) {
        let _ = action;
    }

    /// Drains any pending [`WindowCommand`]s (such as starting a titlebar drag).
    fn take_commands(&mut self) -> Vec<WindowCommand> {
        Vec::new()
    }
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
    /// Extend content under a transparent titlebar with inset traffic lights (macOS).
    pub transparent_titlebar: bool,
    /// Optional native application menu bar.
    pub menu_bar: Option<MenuBar>,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: "rugkit".into(),
            size: Size::new(960.0, 640.0),
            background: Color::WHITE,
            measure_gpu: false,
            transparent_titlebar: false,
            menu_bar: None,
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
    /// Whether the OS asks for reduced motion (macOS: Accessibility › Display › Reduce motion).
    pub reduce_motion: bool,
    /// Current OS appearance (light/dark scheme and system accent colour).
    pub appearance: Appearance,
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

/// Internal user events routed through winit's `EventLoopProxy`.
#[derive(Debug)]
enum UserEvent {
    Wake,
    AccessKit(accesskit_winit::Event),
    Menu(String),
}

impl From<accesskit_winit::Event> for UserEvent {
    fn from(event: accesskit_winit::Event) -> Self {
        Self::AccessKit(event)
    }
}

/// Opens a window and runs `app` until the window is closed.
///
/// Must be called on the main thread. Returns when the window closes.
pub fn run(options: WindowOptions, mut app: impl App) -> Result<(), Error> {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .map_err(|e| Error(format!("event loop: {e}")))?;
    let proxy = event_loop.create_proxy();

    // Forward native menu events into the winit event loop.
    let menu_proxy = proxy.clone();
    muda::MenuEvent::set_event_handler(Some(Arc::new(move |event: muda::MenuEvent| {
        let _ = menu_proxy.send_event(UserEvent::Menu(event.id.0));
    })));

    app.attach(WakeHandle {
        proxy: proxy.clone(),
    });

    let initial_appearance = Appearance::new(ColorScheme::Light, platform::accent_color());

    let mut shell = Shell {
        options,
        app: Box::new(app),
        proxy,
        canvas: Canvas::new(),
        render_cx: RenderContext::new(),
        gpu: None,
        a11y: None,
        _menu: None,
        clock: Clock::default(),
        error: None,
        cursor: None,
        modifiers: Modifiers::default(),
        reduce_motion: platform::reduce_motion(),
        appearance: initial_appearance,
        system_checked: Instant::now(),
        last_ime: ImeState::default(),
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
    proxy: EventLoopProxy<UserEvent>,
    canvas: Canvas,
    render_cx: RenderContext,
    gpu: Option<Gpu>,
    a11y: Option<accesskit_winit::Adapter>,
    _menu: Option<muda::Menu>,
    clock: Clock,
    error: Option<Error>,
    /// Last pointer position in logical pixels.
    cursor: Option<Point>,
    modifiers: Modifiers,
    reduce_motion: bool,
    appearance: Appearance,
    system_checked: Instant,
    last_ime: ImeState,
}

impl Shell {
    fn dispatch(&mut self, event: InputEvent) {
        self.app.event(&event);
        self.drain_commands();
        self.sync_ime();
        if let Some(gpu) = &self.gpu {
            gpu.window.request_redraw();
        }
    }

    fn drain_commands(&mut self) {
        let commands = self.app.take_commands();
        let Some(gpu) = &self.gpu else { return };
        for cmd in commands {
            match cmd {
                WindowCommand::DragWindow => {
                    let _ = gpu.window.drag_window();
                }
                WindowCommand::SetTitle(title) => {
                    gpu.window.set_title(&title);
                }
            }
        }
    }

    fn sync_ime(&mut self) {
        let Some(gpu) = &self.gpu else { return };
        let state = self.app.ime_state().unwrap_or_default();
        if state.enabled != self.last_ime.enabled {
            gpu.window.set_ime_allowed(state.enabled);
        }
        if state.enabled
            && state.cursor_rect != self.last_ime.cursor_rect
            && let Some(r) = state.cursor_rect
        {
            gpu.window.set_ime_cursor_area(
                LogicalPosition::new(r.x0, r.y0),
                LogicalSize::new(r.width().max(1.0), r.height().max(1.0)),
            );
        }
        self.last_ime = state;
    }

    fn push_a11y(&mut self) {
        let Some(a11y) = self.a11y.as_mut() else {
            return;
        };
        let scale = self.gpu.as_ref().map_or(1.0, |g| g.window.scale_factor());
        let app = &mut self.app;
        a11y.update_if_active(|| {
            app.a11y_tree()
                .map(|t| t.to_accesskit(scale))
                .unwrap_or_else(empty_a11y_update)
        });
    }

    fn init_gpu(&mut self, event_loop: &ActiveEventLoop) -> Result<Gpu, Error> {
        let mut attrs = Window::default_attributes()
            .with_title(self.options.title.clone())
            .with_inner_size(LogicalSize::new(
                self.options.size.width,
                self.options.size.height,
            ))
            // AccessKit requires attaching its adapter before the window becomes visible.
            .with_visible(false);

        #[cfg(target_os = "macos")]
        if self.options.transparent_titlebar {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attrs = attrs
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true)
                .with_title_hidden(true);
        }

        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .map_err(|e| Error(format!("window: {e}")))?,
        );

        let a11y = accesskit_winit::Adapter::with_event_loop_proxy(
            event_loop,
            &window,
            self.proxy.clone(),
        );
        self.a11y = Some(a11y);

        // Observe the window's initial light/dark theme before showing it.
        let scheme = match window.theme() {
            Some(WinitTheme::Dark) => ColorScheme::Dark,
            _ => ColorScheme::Light,
        };
        let accent = platform::accent_color();
        self.appearance = Appearance::new(scheme, accent);
        self.app
            .event(&InputEvent::AppearanceChanged(self.appearance));

        if let Some(bar) = &self.options.menu_bar {
            self._menu = menu::install_menu_bar(bar);
        }

        window.set_visible(true);

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
        let frame_start = Instant::now();
        if frame_start - self.system_checked > Duration::from_secs(1) {
            self.reduce_motion = platform::reduce_motion();
            let accent = platform::accent_color();
            if accent != self.appearance.accent {
                self.appearance.accent = accent;
                self.app
                    .event(&InputEvent::AppearanceChanged(self.appearance));
            }
            self.system_checked = frame_start;
        }

        let Some(gpu) = self.gpu.as_mut() else { return };
        let physical = gpu.window.inner_size();
        if physical.width == 0 || physical.height == 0 {
            return;
        }
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
            reduce_motion: self.reduce_motion,
            appearance: self.appearance,
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
        let wants = self.app.wants_frame();
        if wants {
            gpu.window.request_redraw();
        } else {
            self.clock.idle = true;
        }

        self.sync_ime();
        self.push_a11y();
    }
}

impl ApplicationHandler<UserEvent> for Shell {
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

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Wake => {
                if let Some(gpu) = &self.gpu {
                    gpu.window.request_redraw();
                }
            }
            UserEvent::Menu(id) => {
                self.dispatch(InputEvent::MenuAction(id));
            }
            UserEvent::AccessKit(ev) => match ev.window_event {
                accesskit_winit::WindowEvent::InitialTreeRequested => {
                    self.push_a11y();
                }
                accesskit_winit::WindowEvent::ActionRequested(req) => {
                    if let Some(action) = a11y::action_from_accesskit(req) {
                        self.app.a11y_action(&action);
                        self.drain_commands();
                        self.sync_ime();
                        self.push_a11y();
                        if let Some(gpu) = &self.gpu {
                            gpu.window.request_redraw();
                        }
                    }
                }
                accesskit_winit::WindowEvent::AccessibilityDeactivated => {}
            },
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let (Some(a11y), Some(gpu)) = (self.a11y.as_mut(), self.gpu.as_ref()) {
            a11y.process_event(&gpu.window, &event);
        }

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
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.gpu.as_ref().map_or(1.0, |g| g.window.scale_factor());
                let p = Point::new(position.x / scale, position.y / scale);
                self.cursor = Some(p);
                self.dispatch(InputEvent::PointerMove(p));
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                self.dispatch(InputEvent::PointerLeave);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    MouseButton::Left => PointerButton::Primary,
                    MouseButton::Right => PointerButton::Secondary,
                    MouseButton::Middle => PointerButton::Middle,
                    _ => return,
                };
                let Some(p) = self.cursor else { return };
                self.dispatch(match state {
                    ElementState::Pressed => InputEvent::PointerDown(p, button),
                    ElementState::Released => InputEvent::PointerUp(p, button),
                });
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = convert_modifiers(mods.state());
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(ev) = convert_key_event(&event, self.modifiers) {
                    self.dispatch(ev);
                }
            }
            WindowEvent::Ime(ime) => {
                let ev = match ime {
                    Ime::Enabled => ImeEvent::Enabled,
                    Ime::Preedit(text, range) => ImeEvent::Preedit(text, range),
                    Ime::Commit(text) => ImeEvent::Commit(text),
                    Ime::Disabled => ImeEvent::Disabled,
                };
                self.dispatch(InputEvent::Ime(ev));
            }
            WindowEvent::Focused(focused) => {
                self.dispatch(InputEvent::Focused(focused));
            }
            WindowEvent::ThemeChanged(theme) => {
                let scheme = match theme {
                    WinitTheme::Dark => ColorScheme::Dark,
                    WinitTheme::Light => ColorScheme::Light,
                };
                let accent = platform::accent_color();
                self.appearance = Appearance::new(scheme, accent);
                self.dispatch(InputEvent::AppearanceChanged(self.appearance));
            }
            _ => {}
        }
    }
}

fn empty_a11y_update() -> accesskit::TreeUpdate {
    let root = accesskit::NodeId(0);
    let node = accesskit::Node::new(accesskit::Role::Window);
    accesskit::TreeUpdate {
        nodes: vec![(root, node)],
        tree: Some(accesskit::TreeInfo::new(root)),
        tree_id: accesskit::TreeId::ROOT,
        focus: root,
    }
}

fn convert_modifiers(state: ModifiersState) -> Modifiers {
    Modifiers {
        shift: state.shift_key(),
        ctrl: state.control_key(),
        alt: state.alt_key(),
        meta: state.super_key(),
    }
}

fn convert_key_event(event: &KeyEvent, modifiers: Modifiers) -> Option<InputEvent> {
    let key = match &event.logical_key {
        WinitKey::Character(s) => Key::Character(s.to_string()),
        WinitKey::Named(n) => match n {
            NamedKey::Tab => Key::Tab,
            NamedKey::Enter => Key::Enter,
            NamedKey::Escape => Key::Escape,
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Delete => Key::Delete,
            NamedKey::Space => Key::Space,
            NamedKey::ArrowLeft => Key::ArrowLeft,
            NamedKey::ArrowRight => Key::ArrowRight,
            NamedKey::ArrowUp => Key::ArrowUp,
            NamedKey::ArrowDown => Key::ArrowDown,
            NamedKey::Home => Key::Home,
            NamedKey::End => Key::End,
            NamedKey::PageUp => Key::PageUp,
            NamedKey::PageDown => Key::PageDown,
            other => Key::Named(format!("{other:?}")),
        },
        _ => return None,
    };
    match event.state {
        ElementState::Pressed => {
            let text = event
                .text
                .as_ref()
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty());
            Some(InputEvent::KeyDown {
                key,
                modifiers,
                text,
                repeat: event.repeat,
            })
        }
        ElementState::Released => Some(InputEvent::KeyUp { key, modifiers }),
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
    /// The previous frame didn't ask for a follow-up, so the gap before the next one is idle
    /// time, not frame time.
    idle: bool,
}

impl Clock {
    const WINDOW: Duration = Duration::from_millis(500);
    const NOMINAL_FRAME: Duration = Duration::from_micros(16_667);

    fn tick(&mut self, now: Instant) -> (Duration, Duration) {
        let start = *self.start.get_or_insert(now);
        let dt = match self.last {
            None => Duration::ZERO,
            // Coming back from idle: report one nominal frame, so animations that start now
            // don't jump by the whole idle gap.
            Some(_) if self.idle => Self::NOMINAL_FRAME,
            Some(l) => now - l,
        };
        self.idle = false;
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

mod platform {
    use rugkit_render::Color;

    /// Whether the user asked the OS to reduce motion.
    #[cfg(target_os = "macos")]
    pub(crate) fn reduce_motion() -> bool {
        objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
    }

    /// Whether the user asked the OS to reduce motion. Not wired up off macOS yet.
    #[cfg(not(target_os = "macos"))]
    pub(crate) fn reduce_motion() -> bool {
        false
    }

    /// The system accent colour (macOS Settings › Appearance › Accent colour), in sRGB.
    #[cfg(target_os = "macos")]
    pub(crate) fn accent_color() -> Option<Color> {
        let accent = objc2_app_kit::NSColor::controlAccentColor();
        let srgb = objc2_app_kit::NSColorSpace::sRGBColorSpace();
        let converted = accent.colorUsingColorSpace(&srgb)?;
        Some(Color::rgba(
            converted.redComponent() as f32,
            converted.greenComponent() as f32,
            converted.blueComponent() as f32,
            converted.alphaComponent() as f32,
        ))
    }

    /// System accent colour. Not wired up off macOS yet.
    #[cfg(not(target_os = "macos"))]
    pub(crate) fn accent_color() -> Option<Color> {
        None
    }
}
