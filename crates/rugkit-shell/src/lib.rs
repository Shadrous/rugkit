//! Windowing, GPU surface, multi-window, input, accessibility, menus, clipboard and frame loop
//! for rugkit.
//!
//! [`run`] opens a window and calls your [`App`] every frame with a [`Painter`] and
//! [`FrameInfo`]. [`run_windows`] opens multiple windows that share the UI thread (and
//! therefore share reactive signals). winit, wgpu, AccessKit, muda and arboard do the work
//! underneath, and none of their types appear in this API.
//!
//! Redraws happen on demand: after input events, when woken through a [`WakeHandle`], and for
//! as long as the app says it wants frames (see [`App::wants_frame`], e.g. while animations
//! run).

pub mod a11y;
mod appearance;
pub mod clipboard;
mod input;
pub mod menu;

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rugkit_render::backend::vello::util::{RenderContext, RenderSurface};
use rugkit_render::backend::vello::{self, wgpu};
use rugkit_render::{Canvas, Color, Painter, Point, Rect, Size, Vec2};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
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
    /// Closes this window.
    CloseWindow,
}

/// Specification for opening a window (used by [`run_windows`] and [`App::take_new_windows`]).
pub struct WindowSpec {
    /// Window configuration.
    pub options: WindowOptions,
    /// Application state for this window.
    pub app: Box<dyn App>,
}

impl fmt::Debug for WindowSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WindowSpec")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

impl WindowSpec {
    /// Creates a new [`WindowSpec`] from `options` and `app`.
    pub fn new(options: WindowOptions, app: impl App) -> Self {
        Self {
            options,
            app: Box::new(app),
        }
    }
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

    /// Drains any new windows requested to be opened on the shared UI thread.
    fn take_new_windows(&mut self) -> Vec<WindowSpec> {
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
pub fn run(options: WindowOptions, app: impl App) -> Result<(), Error> {
    run_windows(vec![WindowSpec::new(options, app)])
}

/// Opens one or more windows sharing the UI thread (and reactive signals) and runs until all
/// windows are closed.
///
/// Must be called on the main thread.
pub fn run_windows(windows: Vec<WindowSpec>) -> Result<(), Error> {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .map_err(|e| Error(format!("event loop: {e}")))?;
    let proxy = event_loop.create_proxy();

    let menu_proxy = proxy.clone();
    muda::MenuEvent::set_event_handler(Some(Arc::new(move |event: muda::MenuEvent| {
        let _ = menu_proxy.send_event(UserEvent::Menu(event.id.0));
    })));

    let initial_appearance = Appearance::new(ColorScheme::Light, platform::accent_color());

    let mut shell = Shell {
        pending: windows,
        windows: HashMap::new(),
        focused_window: None,
        proxy,
        render_cx: RenderContext::new(),
        _menu: None,
        error: None,
        reduce_motion: platform::reduce_motion(),
        appearance: initial_appearance,
        system_checked: Instant::now(),
    };
    event_loop
        .run_app(&mut shell)
        .map_err(|e| Error(format!("event loop: {e}")))?;
    match shell.error {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

struct WindowState {
    options: WindowOptions,
    app: Box<dyn App>,
    canvas: Canvas,
    window: Arc<Window>,
    surface: RenderSurface<'static>,
    renderer: vello::Renderer,
    a11y: accesskit_winit::Adapter,
    clock: Clock,
    cursor: Option<Point>,
    modifiers: Modifiers,
    last_ime: ImeState,
}

impl WindowState {
    fn sync_ime(&mut self) {
        let state = self.app.ime_state().unwrap_or_default();
        if state.enabled != self.last_ime.enabled {
            self.window.set_ime_allowed(state.enabled);
        }
        if state.enabled
            && state.cursor_rect != self.last_ime.cursor_rect
            && let Some(r) = state.cursor_rect
        {
            self.window.set_ime_cursor_area(
                LogicalPosition::new(r.x0, r.y0),
                LogicalSize::new(r.width().max(1.0), r.height().max(1.0)),
            );
        }
        self.last_ime = state;
    }

    fn push_a11y(&mut self) {
        let scale = self.window.scale_factor();
        let app = &mut self.app;
        self.a11y.update_if_active(|| {
            app.a11y_tree()
                .map(|t| t.to_accesskit(scale))
                .unwrap_or_else(empty_a11y_update)
        });
    }
}

struct Shell {
    pending: Vec<WindowSpec>,
    windows: HashMap<WindowId, WindowState>,
    focused_window: Option<WindowId>,
    proxy: EventLoopProxy<UserEvent>,
    render_cx: RenderContext,
    _menu: Option<muda::Menu>,
    error: Option<Error>,
    reduce_motion: bool,
    appearance: Appearance,
    system_checked: Instant,
}

impl Shell {
    fn open_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        mut spec: WindowSpec,
    ) -> Result<WindowId, Error> {
        spec.app.attach(WakeHandle {
            proxy: self.proxy.clone(),
        });

        let mut attrs = Window::default_attributes()
            .with_title(spec.options.title.clone())
            .with_inner_size(LogicalSize::new(
                spec.options.size.width,
                spec.options.size.height,
            ))
            // AccessKit requires attaching its adapter before the window becomes visible.
            .with_visible(false);

        #[cfg(target_os = "macos")]
        if spec.options.transparent_titlebar {
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
        let id = window.id();

        let a11y = accesskit_winit::Adapter::with_event_loop_proxy(
            event_loop,
            &window,
            self.proxy.clone(),
        );

        let scheme = match window.theme() {
            Some(WinitTheme::Dark) => ColorScheme::Dark,
            _ => ColorScheme::Light,
        };
        let accent = platform::accent_color();
        self.appearance = Appearance::new(scheme, accent);
        spec.app
            .event(&InputEvent::AppearanceChanged(self.appearance));

        if self._menu.is_none()
            && let Some(bar) = &spec.options.menu_bar
        {
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

        window.request_redraw();
        self.windows.insert(
            id,
            WindowState {
                options: spec.options,
                app: spec.app,
                canvas: Canvas::new(),
                window,
                surface,
                renderer,
                a11y,
                clock: Clock::default(),
                cursor: None,
                modifiers: Modifiers::default(),
                last_ime: ImeState::default(),
            },
        );
        if self.focused_window.is_none() {
            self.focused_window = Some(id);
        }
        Ok(id)
    }

    fn dispatch(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: InputEvent) {
        let (commands, new_windows) = {
            let Some(win) = self.windows.get_mut(&id) else {
                return;
            };
            win.app.event(&event);
            win.sync_ime();
            win.window.request_redraw();
            (win.app.take_commands(), win.app.take_new_windows())
        };
        self.run_commands(event_loop, id, commands);
        for spec in new_windows {
            if let Err(err) = self.open_window(event_loop, spec) {
                log::error!("rugkit: failed to open window: {err}");
            }
        }
    }

    fn run_commands(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: WindowId,
        commands: Vec<WindowCommand>,
    ) {
        let mut close = false;
        if let Some(win) = self.windows.get(&id) {
            for cmd in commands {
                match cmd {
                    WindowCommand::DragWindow => {
                        let _ = win.window.drag_window();
                    }
                    WindowCommand::SetTitle(title) => {
                        win.window.set_title(&title);
                    }
                    WindowCommand::CloseWindow => {
                        close = true;
                    }
                }
            }
        }
        if close {
            self.close_window(event_loop, id);
        }
    }

    fn close_window(&mut self, event_loop: &ActiveEventLoop, id: WindowId) {
        self.windows.remove(&id);
        if self.focused_window == Some(id) {
            self.focused_window = self.windows.keys().next().copied();
        }
        if self.windows.is_empty() {
            event_loop.exit();
        }
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop, id: WindowId) {
        let frame_start = Instant::now();
        if frame_start - self.system_checked > Duration::from_secs(1) {
            self.reduce_motion = platform::reduce_motion();
            let accent = platform::accent_color();
            if accent != self.appearance.accent {
                self.appearance.accent = accent;
                let appearance = self.appearance;
                for win in self.windows.values_mut() {
                    win.app.event(&InputEvent::AppearanceChanged(appearance));
                    win.window.request_redraw();
                }
            }
            self.system_checked = frame_start;
        }

        let Some(win) = self.windows.get_mut(&id) else {
            return;
        };
        let physical = win.window.inner_size();
        if physical.width == 0 || physical.height == 0 {
            return;
        }
        let scale = win.window.scale_factor();
        let (time, dt) = win.clock.tick(frame_start);
        let info = FrameInfo {
            size: Size::new(
                f64::from(physical.width) / scale,
                f64::from(physical.height) / scale,
            ),
            scale,
            time,
            dt,
            stats: win.clock.stats,
            reduce_motion: self.reduce_motion,
            appearance: self.appearance,
        };

        let mut painter = win.canvas.begin_frame(scale);
        win.app.draw(&mut painter, &info);

        let handle = &self.render_cx.devices[win.surface.dev_id];
        let params = vello::RenderParams {
            base_color: vello::peniko::Color::new([
                win.options.background.r,
                win.options.background.g,
                win.options.background.b,
                win.options.background.a,
            ]),
            width: win.surface.config.width,
            height: win.surface.config.height,
            antialiasing_method: vello::AaConfig::Area,
        };
        if let Err(e) = win.renderer.render_to_texture(
            &handle.device,
            &handle.queue,
            win.canvas.vello_scene(),
            &win.surface.target_view,
            &params,
        ) {
            log::error!("rugkit: render failed: {e}");
            return;
        }
        let cpu = frame_start.elapsed();
        let gpu_time = if win.options.measure_gpu {
            let _ = handle.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(1)),
            });
            Some(frame_start.elapsed())
        } else {
            None
        };

        let surface_texture = match win.surface.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.render_cx.configure_surface(&win.surface);
                win.window.request_redraw();
                return;
            }
            other => {
                log::warn!("rugkit: skipped a frame: {other:?}");
                win.window.request_redraw();
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
        win.surface.blitter.copy(
            &handle.device,
            &mut encoder,
            &win.surface.target_view,
            &target,
        );
        handle.queue.submit([encoder.finish()]);
        win.window.pre_present_notify();
        handle.queue.present(surface_texture);

        win.clock.record(cpu, gpu_time);
        if win.app.wants_frame() {
            win.window.request_redraw();
        } else {
            win.clock.idle = true;
        }
        win.sync_ime();
        win.push_a11y();

        let new_windows = win.app.take_new_windows();
        let commands = win.app.take_commands();
        self.run_commands(event_loop, id, commands);
        for spec in new_windows {
            if let Err(err) = self.open_window(event_loop, spec) {
                log::error!("rugkit: failed to open window: {err}");
            }
        }
    }
}

impl ApplicationHandler<UserEvent> for Shell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let pending = std::mem::take(&mut self.pending);
        for spec in pending {
            if let Err(err) = self.open_window(event_loop, spec) {
                self.error = Some(err);
                event_loop.exit();
                return;
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Wake => {
                for win in self.windows.values() {
                    win.window.request_redraw();
                }
            }
            UserEvent::Menu(menu_id) => {
                if let Some(id) = self.focused_window {
                    self.dispatch(event_loop, id, InputEvent::MenuAction(menu_id));
                }
            }
            UserEvent::AccessKit(ev) => {
                let id = ev.window_id;
                match ev.window_event {
                    accesskit_winit::WindowEvent::InitialTreeRequested => {
                        if let Some(win) = self.windows.get_mut(&id) {
                            win.push_a11y();
                        }
                    }
                    accesskit_winit::WindowEvent::ActionRequested(req) => {
                        if let Some(action) = a11y::action_from_accesskit(req) {
                            let (commands, new_windows) = {
                                let Some(win) = self.windows.get_mut(&id) else {
                                    return;
                                };
                                win.app.a11y_action(&action);
                                win.sync_ime();
                                win.push_a11y();
                                win.window.request_redraw();
                                (win.app.take_commands(), win.app.take_new_windows())
                            };
                            self.run_commands(event_loop, id, commands);
                            for spec in new_windows {
                                let _ = self.open_window(event_loop, spec);
                            }
                        }
                    }
                    accesskit_winit::WindowEvent::AccessibilityDeactivated => {}
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let Some(win) = self.windows.get_mut(&id) {
            win.a11y.process_event(&win.window, &event);
        }

        match event {
            WindowEvent::CloseRequested => self.close_window(event_loop, id),
            WindowEvent::Resized(size) => {
                if let Some(win) = self.windows.get_mut(&id)
                    && size.width > 0
                    && size.height > 0
                {
                    self.render_cx
                        .resize_surface(&mut win.surface, size.width, size.height);
                    win.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop, id),
            WindowEvent::CursorMoved { position, .. } => {
                let Some(win) = self.windows.get_mut(&id) else {
                    return;
                };
                let scale = win.window.scale_factor();
                let p = Point::new(position.x / scale, position.y / scale);
                win.cursor = Some(p);
                self.dispatch(event_loop, id, InputEvent::PointerMove(p));
            }
            WindowEvent::CursorLeft { .. } => {
                if let Some(win) = self.windows.get_mut(&id) {
                    win.cursor = None;
                }
                self.dispatch(event_loop, id, InputEvent::PointerLeave);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    MouseButton::Left => PointerButton::Primary,
                    MouseButton::Right => PointerButton::Secondary,
                    MouseButton::Middle => PointerButton::Middle,
                    _ => return,
                };
                let Some(p) = self.windows.get(&id).and_then(|w| w.cursor) else {
                    return;
                };
                self.dispatch(
                    event_loop,
                    id,
                    match state {
                        ElementState::Pressed => InputEvent::PointerDown(p, button),
                        ElementState::Released => InputEvent::PointerUp(p, button),
                    },
                );
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let Some(win) = self.windows.get(&id) else {
                    return;
                };
                let scale = win.window.scale_factor();
                let pos = win.cursor.unwrap_or(Point::ZERO);
                let d = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        Vec2::new(f64::from(x) * 28.0, f64::from(y) * 28.0)
                    }
                    MouseScrollDelta::PixelDelta(p) => Vec2::new(p.x / scale, p.y / scale),
                };
                self.dispatch(
                    event_loop,
                    id,
                    InputEvent::Scroll {
                        delta: d,
                        position: pos,
                    },
                );
            }
            WindowEvent::ModifiersChanged(mods) => {
                if let Some(win) = self.windows.get_mut(&id) {
                    win.modifiers = convert_modifiers(mods.state());
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let modifiers = self
                    .windows
                    .get(&id)
                    .map_or(Modifiers::default(), |w| w.modifiers);
                if let Some(ev) = convert_key_event(&event, modifiers) {
                    self.dispatch(event_loop, id, ev);
                }
            }
            WindowEvent::Ime(ime) => {
                let ev = match ime {
                    Ime::Enabled => ImeEvent::Enabled,
                    Ime::Preedit(text, range) => ImeEvent::Preedit(text, range),
                    Ime::Commit(text) => ImeEvent::Commit(text),
                    Ime::Disabled => ImeEvent::Disabled,
                };
                self.dispatch(event_loop, id, InputEvent::Ime(ev));
            }
            WindowEvent::Focused(focused) => {
                if focused {
                    self.focused_window = Some(id);
                }
                self.dispatch(event_loop, id, InputEvent::Focused(focused));
            }
            WindowEvent::ThemeChanged(theme) => {
                let scheme = match theme {
                    WinitTheme::Dark => ColorScheme::Dark,
                    WinitTheme::Light => ColorScheme::Light,
                };
                let accent = platform::accent_color();
                self.appearance = Appearance::new(scheme, accent);
                self.dispatch(
                    event_loop,
                    id,
                    InputEvent::AppearanceChanged(self.appearance),
                );
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
    idle: bool,
}

impl Clock {
    const WINDOW: Duration = Duration::from_millis(500);
    const NOMINAL_FRAME: Duration = Duration::from_micros(16_667);

    fn tick(&mut self, now: Instant) -> (Duration, Duration) {
        let start = *self.start.get_or_insert(now);
        let dt = match self.last {
            None => Duration::ZERO,
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

    #[cfg(target_os = "macos")]
    pub(crate) fn reduce_motion() -> bool {
        objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn reduce_motion() -> bool {
        false
    }

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

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn accent_color() -> Option<Color> {
        None
    }
}
