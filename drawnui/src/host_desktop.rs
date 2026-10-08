//! Native host: a winit window. Windows, Linux and Android draw with OpenGL (glutin, Skia on the
//! default framebuffer; OpenGL ES on Android), macOS and iOS with Metal (a CAMetalLayer on the
//! window's view, Skia on each drawable). iOS and Android add touch input, the safe area and the
//! app lifecycle (the window comes with the first `resumed`; Android loses its surface while in
//! the background).

use std::{
    sync::{
        Arc, Mutex,
        mpsc::{self, Sender},
    },
    time::{Duration, Instant},
};

#[cfg(target_os = "android")]
use android_gpu::Presenter;
#[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "android")))]
use gl::Presenter;
#[cfg(any(target_os = "macos", target_os = "ios"))]
use metal::Presenter;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::{ElementState, Ime, MouseButton as WinitButton, MouseScrollDelta, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{Key, KeyCode, NamedKey, PhysicalKey},
    window::{CursorIcon, Window, WindowAttributes, WindowId},
};

use crate::host_access;
use crate::keyboard::with_key_codes;
use crate::ui::AccessibilityNode;
use crate::{
    App, BakeRequest, ContextMenuSource, Cursor, Decoded, Frame, Gpu, Host, ImageRequest, Images, InputKey, KeyKind, Modifiers, MouseButton,
    PointerKind, RenderingModeType, skia,
};

/// Android: the activity, set by the app's `android_main` (`crate::set_android_app`). Replaced
/// when Android relaunches the activity in the same process (a configuration change the manifest
/// does not take): every activity gets its own `android_main`, event loop and window.
#[cfg(target_os = "android")]
static ANDROID_APP: std::sync::Mutex<Option<crate::AndroidApp>> = std::sync::Mutex::new(None);

#[cfg(target_os = "android")]
pub(crate) fn set_android_app(app: crate::AndroidApp) {
    *ANDROID_APP.lock().unwrap() = Some(app);
    android::forget_field();
}

/// Android: the current activity (image workers read assets through it too).
#[cfg(target_os = "android")]
fn android_app() -> Option<crate::AndroidApp> {
    ANDROID_APP.lock().unwrap().clone()
}

/// How long after a size change frames wait for the GPU before they are presented.
const RESIZE_SYNC: Duration = Duration::from_millis(500);

/// What a worker sends to the event loop: a request id and its answer.
enum Arrived {
    /// A decoded picture (image workers).
    Image(u32, Option<Decoded>),
    /// The bitmap of an ImageDoubleBuffered cache (bake workers).
    Baked(u32, Option<skia::Image>),
    /// A screen reader connected, asked for an action, or left (AccessKit).
    Access(accesskit_winit::Event),
    /// Android: Choreographer's frame callback came, with its vsync time (CLOCK_MONOTONIC ns).
    #[cfg(target_os = "android")]
    Vsync(i64),
    /// macOS 14+, iOS: the display link ticked (a vsync).
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    Tick,
}

impl From<accesskit_winit::Event> for Arrived {
    fn from(event: accesskit_winit::Event) -> Self {
        Arrived::Access(event)
    }
}

// Field order is drop order: everything that may hold GPU objects goes before the GPU context,
// the context before what it draws with (GL context, Metal device and layer), the window last.
struct Desktop {
    app: Box<dyn App>,
    host: Host,
    /// Image requests go to the workers through it.
    images: Sender<ImageRequest>,
    /// ImageDoubleBuffered pictures go to the bake workers through it.
    bakes: Sender<BakeRequest>,
    /// The window's Skia surface. GL keeps one until the size changes (`None` = make it at the
    /// next frame); Metal wraps each frame's drawable and drops it once presented.
    surface: Option<skia::Surface>,
    gpu: Gpu,
    /// `RenderingModeType::Default`: the app draws on the CPU, the frame is shown through the
    /// window's surface.
    cpu: Option<CpuFrames>,
    presenter: Presenter,
    /// Screen readers (UI Automation, NSAccessibility, AT-SPI): made with the first `resumed`,
    /// before the window is shown; dropped before the window.
    access: Option<accesskit_winit::Adapter>,
    /// The accessibility snapshot last sent to it, and the node its focus is on.
    access_nodes: Vec<AccessibilityNode>,
    access_focus: accesskit::NodeId,
    /// For the screen-reader adapter (not made on Android yet).
    #[cfg_attr(target_os = "android", allow(dead_code))]
    proxy: EventLoopProxy<Arrived>,
    window: Window,
    /// Minimized or covered: no frames (Metal's `nextDrawable` would block up to a second).
    occluded: bool,
    /// In the background (iOS): no GPU work at all, the system kills an app that submits any.
    suspended: bool,
    /// The finger that drives the pointer (iOS, Android); other fingers are ignored.
    #[cfg(any(target_os = "ios", target_os = "android"))]
    touch: Option<u64>,
    /// The safe area last reported to the app, points (iOS, Android).
    #[cfg(any(target_os = "ios", target_os = "android"))]
    safe_area: crate::Thickness,
    /// Android: the insets were read since the last resize (they come through JNI, not per frame).
    #[cfg(target_os = "android")]
    insets_read: bool,
    /// Android: frames come from Choreographer (C# Super.Android): a frame callback is asked for,
    /// and a frame is wanted at the next one.
    #[cfg(target_os = "android")]
    frame_posted: bool,
    #[cfg(target_os = "android")]
    frame_wanted: bool,
    /// Android: an editor has the soft keyboard; its text field is read after every event.
    #[cfg(target_os = "android")]
    ime_open: bool,
    /// Android: the vsync time of the frame to draw, CLOCK_MONOTONIC nanoseconds.
    #[cfg(target_os = "android")]
    vsync_ns: Option<i64>,
    /// Android: the vsync period, the shortest of the last callback intervals, and those times.
    #[cfg(target_os = "android")]
    vsync: android::VsyncClock,
    /// macOS 14+ and iOS: frames come from a display link (C# Super.iOS / Super.Mac), running
    /// while a frame is wanted, at the display's rate (120 Hz on ProMotion) or `crate::max_fps`.
    /// (On iOS winit's `request_redraw` is UIKit's `drawRect`: 60 Hz at most, and a request made
    /// inside a frame is dropped.) `None` below macOS 14: winit's redraws, as before.
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    ticker: Option<ticker::Ticker>,
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    frame_wanted: bool,
    start: Instant,
    /// When the app wants a frame although it asked for none (a timer).
    wake: Option<Instant>,
    /// When the window last changed size.
    resized_at: Option<Instant>,
    cursor: (f32, f32),
    /// The button of the press: one the window may still lose. Other buttons wait for it.
    pressed: Option<MouseButton>,
    modifiers: Modifiers,
    /// An input method is composing (Chinese, Japanese): its keys are its own.
    ime_composing: bool,
    stats: Stats,
    /// Windows, Linux (OpenGL): frames spaced by the host where the driver ignores vsync.
    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "android")))]
    pacing: Pacing,
    /// Android: the window's last size; a resize of the same size is a change of the insets
    /// (the surface is not waited for).
    #[cfg(target_os = "android")]
    size: winit::dpi::PhysicalSize<u32>,
}

/// `RenderingModeType::Default` (DrawnUI's software canvas): the engine draws on the CPU, its
/// caches are CPU bitmaps; each frame is then drawn onto the window's GPU surface.
struct CpuFrames {
    gpu: Gpu,
    surface: Option<skia::Surface>,
}

impl CpuFrames {
    /// The frame's pixels (made again when the window's size changed) and the CPU engine.
    fn frame(&mut self, width: i32, height: i32) -> Option<(&mut skia::Surface, &mut Gpu)> {
        if self.surface.as_ref().is_none_or(|s| s.width() != width || s.height() != height) {
            self.surface = skia::surfaces::raster_n32_premul((width, height));
        }
        Some((self.surface.as_mut()?, &mut self.gpu))
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        // Vulkan: what Skia frees may still be in use on the GPU.
        #[cfg(target_os = "android")]
        self.presenter.wait_idle();
        // Avoids a crash on exit with some AMD drivers (rust-skia #1235).
        self.gpu.abandon();
    }
}

pub fn run(title: &str, size: (f64, f64), make: fn() -> Box<dyn App>) {
    let mut builder = EventLoop::<Arrived>::with_user_event();
    #[cfg(target_os = "android")]
    {
        use winit::platform::android::EventLoopBuilderExtAndroid;
        android::log_panics();
        builder.with_android_app(android_app().expect("drawnui::set_android_app before run"));
    }
    let event_loop = builder.build().expect("no event loop");
    // Hidden until the screen-reader adapter is made (it must come before the window shows).
    let attributes = WindowAttributes::default().with_title(title).with_visible(false);
    // iOS, Android: the window is the screen (winit would make a UIWindow of the size asked for).
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let attributes = attributes.with_inner_size(LogicalSize::new(size.0, size.1));
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let _ = size;
    event_loop.set_control_flow(ControlFlow::Wait);
    // iOS and Android make windows only once the app runs: the window comes with the first `resumed`.
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let mut launch = ios::Launch { attributes: Some(attributes), make, proxy: event_loop.create_proxy(), desktop: None };
        event_loop.run_app(&mut launch).expect("event loop failed");
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        #[cfg(not(target_os = "macos"))]
        let (window, presenter) = Presenter::new(&event_loop, attributes);
        #[cfg(target_os = "macos")]
        #[allow(deprecated)]
        let (window, presenter) = Presenter::new(event_loop.create_window(attributes).expect("no window"), skia::Color::TRANSPARENT);
        let mut desktop = Desktop::new(window, presenter, make(), event_loop.create_proxy());
        event_loop.run_app(&mut desktop).expect("event loop failed");
    }
}

impl Desktop {
    fn new(window: Window, presenter: Presenter, mut app: Box<dyn App>, proxy: EventLoopProxy<Arrived>) -> Self {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let ticker = ticker::Ticker::new(proxy.clone(), &window);
        #[cfg(windows)]
        exe_icon(&window);
        #[cfg(target_os = "linux")]
        file_icon(&window);
        let gpu = presenter.gpu();
        #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "android")))]
        let pacing = Pacing::new(&window);

        // The desktop has no keyboard navigation but ours: Tab, arrows in groups, Enter / Space,
        // Escape, the focus ring; it needs the accessibility snapshot, also for screen readers.
        let mut host = Host { bake_workers: true, keyboard_navigation: true, accessibility_on: true, ..Host::default() };
        app.init(&mut host);
        #[cfg(target_os = "android")]
        let size = window.inner_size();
        let cpu = (app.rendering_mode() == RenderingModeType::Default).then(|| CpuFrames { gpu: Gpu::raster(), surface: None });

        let mut desktop = Desktop {
            app,
            host,
            images: image_workers(proxy.clone()),
            bakes: bake_workers(proxy.clone()),
            surface: None,
            gpu,
            cpu,
            presenter,
            access: None,
            access_nodes: Vec::new(),
            access_focus: host_access::ROOT,
            proxy,
            window,
            occluded: false,
            suspended: false,
            #[cfg(any(target_os = "ios", target_os = "android"))]
            touch: None,
            #[cfg(any(target_os = "ios", target_os = "android"))]
            safe_area: crate::Thickness::default(),
            #[cfg(target_os = "android")]
            insets_read: false,
            #[cfg(target_os = "android")]
            frame_posted: false,
            #[cfg(target_os = "android")]
            frame_wanted: false,
            #[cfg(target_os = "android")]
            ime_open: false,
            #[cfg(target_os = "android")]
            vsync_ns: None,
            #[cfg(target_os = "android")]
            vsync: android::VsyncClock::new(),
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            ticker,
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            frame_wanted: false,
            start: Instant::now(),
            wake: None,
            resized_at: None,
            cursor: (0.0, 0.0),
            pressed: None,
            modifiers: Modifiers::default(),
            ime_composing: false,
            stats: Stats::default(),
            #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "android")))]
            pacing,
            #[cfg(target_os = "android")]
            size,
        };
        #[cfg(any(target_os = "ios", target_os = "android"))]
        desktop.report_safe_area();
        desktop.deliver_assets();
        desktop.redraw();
        desktop
    }

    /// What a screen reader asked for (AccessKit). Connecting turns the snapshot on (the app builds
    /// it only while someone reads it); Click is the overlay's activation, a tap at the node's
    /// center (UI Automation Invoke, VoiceOver press).
    fn access_event(&mut self, event: accesskit_winit::WindowEvent) {
        use accesskit_winit::WindowEvent as Access;
        match event {
            Access::InitialTreeRequested => self.send_access_tree(),
            Access::ActionRequested(request) => {
                let id = host_access::snapshot_id(request.target_node);
                match (request.action, id) {
                    (accesskit::Action::Click, Some(id)) => self.app.accessibility_activate(id),
                    (accesskit::Action::Focus, Some(id)) => {
                        if let Some(previous) = host_access::snapshot_id(self.access_focus) {
                            self.app.accessibility_focus(previous, false);
                        }
                        self.access_focus = request.target_node;
                        self.app.accessibility_focus(id, true);
                        self.send_access_tree();
                    }
                    (accesskit::Action::Increment, Some(id)) => self.app.accessibility_adjust(id, true),
                    (accesskit::Action::Decrement, Some(id)) => self.app.accessibility_adjust(id, false),
                    (accesskit::Action::ScrollIntoView, Some(id)) => self.app.accessibility_scroll_into_view(id),
                    // AccessKit names the way the view moves: ScrollDown shows what is below.
                    (accesskit::Action::ScrollDown, Some(id)) => self.app.accessibility_scroll(id, 0.0, 1.0),
                    (accesskit::Action::ScrollUp, Some(id)) => self.app.accessibility_scroll(id, 0.0, -1.0),
                    (accesskit::Action::ScrollRight, Some(id)) => self.app.accessibility_scroll(id, 1.0, 0.0),
                    (accesskit::Action::ScrollLeft, Some(id)) => self.app.accessibility_scroll(id, -1.0, 0.0),
                    (accesskit::Action::SetValue, Some(id)) => {
                        if let Some(accesskit::ActionData::NumericValue(value)) = request.data {
                            self.app.accessibility_set_value(id, value);
                        }
                    }
                    _ => {}
                }
            }
            // The snapshot stays on: the keyboard navigation reads it too.
            Access::AccessibilityDeactivated => {}
        }
    }

    /// The whole tree to the adapter, when a screen reader is connected.
    fn send_access_tree(&mut self) {
        let (title, scale, focus, nodes) = (self.window.title(), self.window.scale_factor(), self.access_focus, &self.access_nodes);
        if let Some(access) = &mut self.access {
            access.update_if_active(|| host_access::tree(&title, nodes, scale, focus));
        }
    }

    /// Asks for a frame.
    fn redraw(&mut self) {
        // macOS 14+, iOS: the frame is drawn at the display link's next tick.
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        if let Some(ticker) = &self.ticker {
            self.frame_wanted = true;
            ticker.run(!self.suspended);
            return;
        }
        // Android: the frame waits for the next vsync (Choreographer), as C# Super.Android does.
        #[cfg(target_os = "android")]
        {
            self.frame_wanted = true;
            if !self.frame_posted && !self.suspended {
                self.frame_posted = android::post_frame(&self.proxy);
                if !self.frame_posted {
                    self.frame_wanted = false;
                    self.window.request_redraw();
                }
            }
            return;
        }
        // A driver that ignores vsync: the frame waits for one refresh after the last one.
        #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "android")))]
        if let Some(at) = self.pacing.hold(Instant::now()) {
            self.pacing.due = Some(at);
            return;
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        self.window.request_redraw();
    }

    /// The size of what is drawn, pixels: the whole window. On iOS winit's `inner_size` is the
    /// safe area; the app draws under the status bar and the home indicator, and is told the
    /// safe area (`App::safe_insets`) to keep its content out of them.
    fn surface_size(&self) -> winit::dpi::PhysicalSize<u32> {
        #[cfg(target_os = "ios")]
        return self.window.outer_size();
        #[cfg(not(target_os = "ios"))]
        self.window.inner_size()
    }

    /// Android: the safe area, the system bars and the display cutout as insets in points (the
    /// window is edge to edge), given to the app when it changes. Read once after each resize or
    /// resume; until the decor view has its insets, at every frame.
    #[cfg(target_os = "android")]
    fn report_safe_area(&mut self) {
        if self.insets_read {
            return;
        }
        let Some([left, top, right, bottom]) = android::system_insets() else { return };
        self.insets_read = true;
        let scale = self.window.scale_factor();
        let points = |px: i32| (px.max(0) as f64 / scale) as f32;
        let insets = crate::Thickness { left: points(left), top: points(top), right: points(right), bottom: points(bottom) };
        if insets != self.safe_area {
            android::log(&format!("drawnui: safe area {insets:?} points (scale {scale})"));
            self.safe_area = insets;
            self.app.safe_insets(insets);
            self.redraw();
        }
    }

    /// iOS: the safe area as insets in points, given to the app when it changes (start, rotation).
    #[cfg(target_os = "ios")]
    fn report_safe_area(&mut self) {
        let scale = self.window.scale_factor();
        let (Ok(safe), Ok(full)) = (self.window.inner_position(), self.window.outer_position()) else { return };
        let (safe_size, full_size) = (self.window.inner_size(), self.window.outer_size());
        let left = (safe.x - full.x).max(0) as f64;
        let top = (safe.y - full.y).max(0) as f64;
        let right = (full_size.width as f64 - safe_size.width as f64 - left).max(0.0);
        let bottom = (full_size.height as f64 - safe_size.height as f64 - top).max(0.0);
        let insets = crate::Thickness {
            left: (left / scale) as f32,
            top: (top / scale) as f32,
            right: (right / scale) as f32,
            bottom: (bottom / scale) as f32,
        };
        if insets != self.safe_area {
            self.safe_area = insets;
            self.app.safe_insets(insets);
            self.redraw();
        }
    }
}

/// The threads that read and decode image files, so the frame thread never does: one per spare
/// core, at most as many as the manager has requests out (`Images::MAX_IN_FLIGHT`), which also
/// keeps the queue that short. Every answer wakes the event loop: an idle app gets the frame
/// that shows the picture.
fn image_workers(proxy: EventLoopProxy<Arrived>) -> Sender<ImageRequest> {
    let (requests, queue) = mpsc::channel::<ImageRequest>();
    let queue = Arc::new(Mutex::new(queue));
    let spare = std::thread::available_parallelism().map_or(1, |cores| cores.get() - 1);
    for _ in 0..spare.clamp(1, Images::MAX_IN_FLIGHT) {
        let (queue, proxy) = (queue.clone(), proxy.clone());
        std::thread::spawn(move || {
            loop {
                // The sender is gone with the window.
                let Ok(request) = queue.lock().expect("an image worker panicked").recv() else { break };
                // A source is a URL: what follows `#` is not part of the file name.
                let path = request.source.split('#').next().unwrap_or_default();
                let decoded = match read_app_file(path) {
                    Ok(bytes) if request.frames => Images::decode_frames(&bytes),
                    Ok(bytes) => Images::decode(&bytes, request.width, request.height),
                    Err(e) => {
                        eprintln!("drawnui: image {path}: {e}");
                        None
                    }
                };
                if proxy.send_event(Arrived::Image(request.id, decoded)).is_err() {
                    break;
                }
            }
        });
    }
    requests
}

/// The threads that draw ImageDoubleBuffered pictures into bitmaps, so the frame thread does not
/// (DrawnUI OffscreenBake workers: half the cores, 2 to 4). Every answer wakes the event loop.
fn bake_workers(proxy: EventLoopProxy<Arrived>) -> Sender<BakeRequest> {
    let (requests, queue) = mpsc::channel::<BakeRequest>();
    let queue = Arc::new(Mutex::new(queue));
    let cores = std::thread::available_parallelism().map_or(2, |cores| cores.get());
    for _ in 0..(cores / 2).clamp(2, 4) {
        let (queue, proxy) = (queue.clone(), proxy.clone());
        std::thread::spawn(move || {
            loop {
                // The sender is gone with the window.
                let Ok(request) = queue.lock().expect("a bake worker panicked").recv() else { break };
                if proxy.send_event(Arrived::Baked(request.id, request.bake())).is_err() {
                    break;
                }
            }
        });
    }
    requests
}

impl Desktop {
    /// The clock of frames and input events alike.
    fn now_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    fn deliver_assets(&mut self) {
        for (id, url) in std::mem::take(&mut self.host.requests) {
            match read_app_file(&url) {
                Ok(bytes) => self.app.asset(id, bytes),
                Err(e) => {
                    eprintln!("drawnui: asset {url}: {e}");
                    self.app.asset(id, Vec::new());
                }
            }
            self.redraw();
        }
        for request in std::mem::take(&mut self.host.images) {
            // The workers end only with the window.
            let _ = self.images.send(request);
        }
        for request in self.host.bakes.drain(..) {
            let _ = self.bakes.send(request);
        }
    }

    fn draw(&mut self) {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        if self.occluded {
            return;
        }
        if self.suspended {
            return;
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        self.report_safe_area();
        let (size, clock) = (self.surface_size(), self.start);
        // Metal: taking the next drawable waits for the compositor (about a vsync when frames run
        // back to back), so what needs no drawable runs first, or the wait adds to the frame and
        // it is shown a vsync late (drawnui-cross 6k, C# 416af16e). The wait is not frame work.
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let (prepared_at, prepared_ms) = {
            let at = Instant::now();
            let time_ms = (at - clock).as_secs_f64() * 1000.0;
            let scale = self.window.scale_factor() as f32;
            self.app.prepare(size.width as f32, size.height as f32, scale, time_ms);
            (at, time_ms)
        };
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let waiting = Instant::now();
        let Some(surface) = self.presenter.begin(&mut self.gpu, &mut self.surface, size.width.max(1), size.height.max(1)) else {
            // No drawable (the window is hidden): the next event that wants a frame asks again.
            return;
        };
        let frame_start = Instant::now();
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        let time_ms = (frame_start - clock).as_secs_f64() * 1000.0;
        // The frame runs at the time it was prepared at; its work is counted without the wait.
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let (frame_start, time_ms) = (prepared_at + (frame_start - waiting), prepared_ms);
        // Paced frames: animations step with the frame's slot on the refresh grid, not with the
        // moment the wake-up came (a timer is late by up to a millisecond).
        #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "android")))]
        let time_ms = self.pacing.frame(frame_start).map_or(time_ms, |slot| (slot - clock).as_secs_f64() * 1000.0);
        // Android: animations step with the vsync the frame is shown on (the callback's vsync plus
        // one period), not with the time this frame happens to start: scheduling noise would be
        // movement jitter (C# Super.Android VsyncFrameTimeNanos).
        #[cfg(target_os = "android")]
        let time_ms = self.vsync_ns.take().map_or(time_ms, |ns| self.vsync.frame_ms(ns, time_ms));
        let (width, height, scale) = (size.width as f32, size.height as f32, self.window.scale_factor() as f32);
        let more = match &mut self.cpu {
            None => self.app.frame(&mut Frame { surface, gpu: &mut self.gpu, width, height, scale, time_ms, host: &mut self.host }),
            Some(cpu) => {
                let Some((pixels, gpu)) = cpu.frame(surface.width(), surface.height()) else { return };
                let more = self.app.frame(&mut Frame { surface: pixels, gpu, width, height, scale, time_ms, host: &mut self.host });
                let mut paint = skia::Paint::default();
                paint.set_blend_mode(skia::BlendMode::Src);
                pixels.draw(surface.canvas(), (0, 0), skia::SamplingOptions::default(), Some(&paint));
                more
            }
        };
        self.presenter.end_frame(&mut self.gpu, &mut self.surface);
        // A new accessibility snapshot (built only while a screen reader is connected).
        if let Some(nodes) = self.host.accessibility.take() {
            self.access_focus = host_access::refocus(&nodes, self.access_focus, height / scale.max(0.1));
            self.access_nodes = nodes;
            self.send_access_tree();
        }
        // While the window is resized, a frame is presented only once the GPU ran it: presented
        // with work still on the GPU, it made the next resize wait 0.8 to 2.6 s on Windows (DWM;
        // Intel Arc), the window lagged behind the mouse. Other frames keep the GPU running behind.
        if self.resized_at.is_some_and(|at| at.elapsed() < RESIZE_SYNC) {
            self.gpu.finish();
        }
        self.stats.frame(frame_start, time_ms);
        // A GPU reset lost the context: a new one, and the next frame draws everything again
        // (`Ui` drops what lived on the old one, DrawnUI GraphicContextMismatch).
        if self.gpu.lost() {
            self.recreate_gpu();
            self.redraw();
            return;
        }
        self.presenter.present(&mut self.surface);
        self.deliver_assets();
        self.apply_input_requests();
        let wake = self.host.wake_ms.take().filter(|_| !more);
        self.wake = wake.map(|ms| self.start + Duration::from_secs_f64(ms / 1000.0));
        if more {
            self.redraw();
        }
    }
}

impl Desktop {
    /// A new GPU context on the same window after the GPU lost the old one; the window surface
    /// is made again at the next frame.
    fn recreate_gpu(&mut self) {
        eprintln!("drawnui: the GPU context was lost, making a new one");
        self.surface = None;
        let gpu = self.presenter.gpu_after(&self.window, &mut self.gpu);
        self.gpu = gpu;
    }

    /// The cursor and the IME follow what the app asked for during the frame.
    fn apply_input_requests(&mut self) {
        for url in self.host.urls.drain(..) {
            open_url(&url);
        }
        if let Some(text) = self.host.clipboard.take() {
            clipboard::set(&text);
        }
        if std::mem::take(&mut self.host.paste)
            && let Some(text) = clipboard::get()
        {
            self.key(KeyKind::Char, "", &text, false);
        }
        if let Some(cursor) = self.host.cursor.take() {
            self.window.set_cursor(match cursor {
                Cursor::Default => CursorIcon::Default,
                Cursor::Pointer => CursorIcon::Pointer,
                Cursor::Text => CursorIcon::Text,
            });
        }
        if let Some(area) = self.host.text_input.take() {
            // The IME composes over the focused editor; games and plain controls get raw keys.
            self.window.set_ime_allowed(area.is_some());
            if let Some(r) = area {
                self.window.set_ime_cursor_area(LogicalPosition::new(r.left, r.top), LogicalSize::new(r.width(), r.height()));
            }
            #[cfg(target_os = "android")]
            {
                self.ime_open = area.is_some();
                if self.ime_open {
                    android::open_text_input();
                }
            }
        }
    }

    /// Android: what the soft keyboard did to its text field (GameTextInput, which winit does not
    /// read) reaches the editor as the web host's hidden textarea sends it: deleted characters as
    /// Backspace, typed text as `KeyKind::Char`, a line break as Enter. A composition (Chinese,
    /// Japanese) comes when it is committed.
    #[cfg(target_os = "android")]
    fn take_soft_keyboard_text(&mut self) {
        let Some((deleted, typed)) = android::take_text_input() else { return };
        for _ in 0..deleted {
            self.key(KeyKind::Down, "Backspace", "", false);
            self.key(KeyKind::Up, "Backspace", "", false);
        }
        for (i, line) in typed.split('\n').enumerate() {
            if i > 0 {
                self.key(KeyKind::Down, "Enter", "", false);
                self.key(KeyKind::Up, "Enter", "", false);
            }
            if !line.is_empty() {
                self.key(KeyKind::Char, "", line, false);
            }
        }
    }

    fn key(&mut self, kind: KeyKind, key: InputKey, text: &str, repeat: bool) -> bool {
        let used = self.app.key(kind, key, text, self.modifiers, repeat);
        self.sync_access_focus();
        self.redraw();
        used
    }

    /// A screen reader follows the keyboard: its focus is the node the keyboard is on.
    fn sync_access_focus(&mut self) {
        let focus = self.app.accessibility_focused_id().map_or(host_access::ROOT, host_access::node_id);
        if focus != self.access_focus {
            self.access_focus = focus;
            self.send_access_tree();
        }
    }
}

/// The exe's own icon (icon resource 1, which an app's build script embeds) on the title bar and
/// the taskbar, at the sizes the window's scale wants, as .NET does with `ApplicationIcon`. An exe
/// without one keeps the system default.
// ponytail: loaded once at the start scale; reload on ScaleFactorChanged if a move between
// monitors of different scale ever shows a blurry icon.
#[cfg(windows)]
fn exe_icon(window: &Window) {
    use winit::platform::windows::{IconExtWindows, WindowExtWindows};
    let icon = |points: f64| {
        let px = (points * window.scale_factor()).round() as u32;
        winit::window::Icon::from_resource(1, Some(winit::dpi::PhysicalSize::new(px, px))).ok()
    };
    window.set_window_icon(icon(16.0));
    window.set_taskbar_icon(icon(32.0));
}

/// Linux: the window icon from the app's `icon.ico` (the file Windows embeds as the exe's icon),
/// next to the exe or in the working folder; Skia decodes its largest picture. X11 shows it on the
/// window and the task list; Wayland has no window icons (the desktop takes the icon of the app's
/// .desktop entry).
#[cfg(target_os = "linux")]
fn file_icon(window: &Window) {
    let Ok(bytes) = read_app_file("icon.ico") else { return };
    let Some(image) = skia::Image::from_encoded(skia::Data::new_copy(&bytes)) else { return };
    let (width, height) = (image.width(), image.height());
    let info = skia::ImageInfo::new((width, height), skia::ColorType::RGBA8888, skia::AlphaType::Unpremul, None);
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    if image.read_pixels(&info, &mut rgba, width as usize * 4, (0, 0), skia::image::CachingHint::Disallow) {
        window.set_window_icon(winit::window::Icon::from_rgba(rgba, width as u32, height as u32).ok());
    }
}

/// Reads a file of the app. A relative path is the app's: next to the exe first, as .NET reads
/// from the app's base folder (an app started from Explorer or a shortcut has another working
/// folder), then the working folder (`cargo run` from the app's folder).
#[cfg(not(target_os = "android"))]
fn read_app_file(path: &str) -> std::io::Result<Vec<u8>> {
    let beside_exe = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join(path)));
    match beside_exe {
        Some(file) if file.is_file() => std::fs::read(file),
        _ => std::fs::read(path),
    }
}

/// Android: a file of the app is an asset of the APK (`assets/<path>`).
#[cfg(target_os = "android")]
fn read_app_file(path: &str) -> std::io::Result<Vec<u8>> {
    use std::io::{Error, ErrorKind, Read};
    let app = android_app().ok_or_else(|| Error::new(ErrorKind::NotFound, "no activity"))?;
    let name = std::ffi::CString::new(path).map_err(|e| Error::new(ErrorKind::InvalidInput, e))?;
    let mut asset = app.asset_manager().open(&name).ok_or_else(|| Error::new(ErrorKind::NotFound, path.to_owned()))?;
    let mut bytes = Vec::new();
    asset.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Android: what the app prints goes nowhere; panics and the host's lines go to logcat.
/// Frames come from Choreographer.
#[cfg(target_os = "android")]
mod android {
    use std::ffi::{CString, c_char, c_int, c_long, c_void};
    use std::sync::OnceLock;

    use winit::event_loop::EventLoopProxy;
    use winit::platform::android::activity::input::{ImeOptions, InputType, TextInputAction, TextInputState, TextSpan};

    #[repr(C)]
    struct AChoreographer {
        _private: [u8; 0],
    }

    #[repr(C)]
    struct Timespec {
        seconds: c_long,
        nanoseconds: c_long,
    }

    type Post64 = unsafe extern "C" fn(*mut AChoreographer, unsafe extern "C" fn(i64, *mut c_void), *mut c_void);

    unsafe extern "C" {
        fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
        fn AChoreographer_getInstance() -> *mut AChoreographer;
        /// API 24; its time is a `long` (32 bits on armv7): only before API 29.
        fn AChoreographer_postFrameCallback(choreographer: *mut AChoreographer, callback: unsafe extern "C" fn(c_long, *mut c_void), data: *mut c_void);
        fn dlopen(name: *const c_char, flags: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn clock_gettime(clock: c_int, time: *mut Timespec) -> c_int;
    }

    thread_local! {
        /// Where frame callbacks go. A user event: winit's Android loop ignores a bare looper
        /// wake that brings no redraw or user event. Callbacks run on the thread that posted them,
        /// `android_main`'s: one per activity, each with its own event loop.
        static PROXY: std::cell::OnceCell<EventLoopProxy<super::Arrived>> = const { std::cell::OnceCell::new() };
    }

    fn on_vsync(nanos: i64) {
        PROXY.with(|proxy| {
            if let Some(proxy) = proxy.get() {
                let _ = proxy.send_event(super::Arrived::Vsync(nanos));
            }
        });
    }

    unsafe extern "C" fn on_frame64(nanos: i64, _data: *mut c_void) {
        on_vsync(nanos);
    }

    unsafe extern "C" fn on_frame(nanos: c_long, _data: *mut c_void) {
        on_vsync(nanos as i64);
    }

    /// Asks Choreographer for a callback at the next vsync; false when there is no Choreographer.
    pub(super) fn post_frame(proxy: &EventLoopProxy<super::Arrived>) -> bool {
        static POST64: OnceLock<Option<Post64>> = OnceLock::new();
        // SAFETY: dlsym of a libandroid function of this signature (API 29+); absent before.
        let post64 = *POST64.get_or_init(|| unsafe {
            let library = dlopen(c"libandroid.so".as_ptr(), 2);
            let symbol = if library.is_null() { std::ptr::null_mut() } else { dlsym(library, c"AChoreographer_postFrameCallback64".as_ptr()) };
            (!symbol.is_null()).then(|| std::mem::transmute::<*mut c_void, Post64>(symbol))
        });
        PROXY.with(|cell| {
            cell.get_or_init(|| proxy.clone());
        });
        // SAFETY: called on android_main's thread, which has the looper Choreographer needs.
        unsafe {
            let choreographer = AChoreographer_getInstance();
            if choreographer.is_null() {
                return false;
            }
            match post64 {
                Some(post) => post(choreographer, on_frame64, std::ptr::null_mut()),
                None => AChoreographer_postFrameCallback(choreographer, on_frame, std::ptr::null_mut()),
            }
        }
        true
    }

    /// What the soft keyboard's text field starts with: zero-width spaces, the caret after them.
    /// Typing appends to them and Backspace deletes them, so every edit is seen as a change; they
    /// are no spaces, so the keyboard's own rules (two spaces make ". ") never act on them. The
    /// field is not reset after every edit: GameTextInput restarts the input method on each
    /// state it is given (one restart per key when the field was reset after each).
    const PLACEHOLDER: &str = "\u{200B}\u{200B}\u{200B}\u{200B}\u{200B}\u{200B}\u{200B}\u{200B}";
    /// The field goes back to the placeholder when fewer of it are left, or when it holds this much.
    const PLACEHOLDER_LOW: usize = 2;
    const FIELD_MAX: usize = 200;

    /// The field's text after the last edit taken (empty: not set yet).
    static FIELD: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

    /// A new activity: its text field is not set yet.
    pub(super) fn forget_field() {
        FIELD.lock().unwrap().clear();
    }

    fn reset_field(app: &crate::AndroidApp) {
        let caret = PLACEHOLDER.chars().count();
        app.set_text_input_state(TextInputState {
            text: PLACEHOLDER.into(),
            selection: TextSpan { start: caret, end: caret },
            compose_region: None,
        });
        *FIELD.lock().unwrap() = PLACEHOLDER.into();
    }

    /// An editor took the keyboard: a plain text field without suggestions (the web textarea's
    /// autocorrect off), multi-line so that Enter comes as a line break.
    pub(super) fn open_text_input() {
        let Some(app) = super::android_app() else { return };
        let kind = InputType::TYPE_CLASS_TEXT | InputType::TYPE_TEXT_FLAG_MULTI_LINE | InputType::TYPE_TEXT_FLAG_NO_SUGGESTIONS;
        // `IMG_`: android-activity's spelling of IME_FLAG_NO_EXTRACT_UI.
        app.set_ime_editor_info(kind, TextInputAction::None, ImeOptions::IME_FLAG_NO_FULLSCREEN | ImeOptions::IMG_FLAG_NO_EXTRACT_UI);
        reset_field(&app);
    }

    /// What the keyboard did since the last call, as characters deleted before the caret, then
    /// text typed; nothing while the field is unchanged or an input method composes.
    pub(super) fn take_text_input() -> Option<(usize, String)> {
        let app = super::android_app()?;
        let state = app.text_input_state();
        let mut field = FIELD.lock().unwrap();
        if state.compose_region.is_some() || field.is_empty() || state.text == *field {
            return None;
        }
        // The common start, then what of the old text is gone and what is new.
        let common = field.chars().zip(state.text.chars()).take_while(|(a, b)| a == b).count();
        let deleted = field.chars().count() - common;
        let typed: String = state.text.chars().skip(common).collect();
        *field = state.text.clone();
        let left = field.chars().take_while(|&c| c == '\u{200B}').count();
        // The caret away from the end (the keyboard moved it): the next edit would not be at the
        // editor's caret, so the field starts over.
        let caret_at_end = state.selection.start == state.selection.end && state.selection.end == field.encode_utf16().count();
        drop(field);
        if left < PLACEHOLDER_LOW || state.text.chars().count() > FIELD_MAX || !caret_at_end {
            reset_field(&app);
        }
        Some((deleted, typed))
    }

    /// CLOCK_MONOTONIC (System.nanoTime, Choreographer's clock), nanoseconds.
    fn monotonic_ns() -> i64 {
        let mut time = Timespec { seconds: 0, nanoseconds: 0 };
        // SAFETY: a valid timespec; 1 = CLOCK_MONOTONIC.
        unsafe { clock_gettime(1, &mut time) };
        time.seconds as i64 * 1_000_000_000 + time.nanoseconds as i64
    }

    /// Frame times from vsyncs, on the host's clock (milliseconds since it started).
    pub(super) struct VsyncClock {
        /// CLOCK_MONOTONIC when the host's clock read 0 (taken with it).
        origin_ns: i64,
        /// The last callback intervals: the shortest is the period (skipped vsyncs are longer).
        intervals: [i64; 16],
        next: usize,
        last_vsync: i64,
        last_ms: f64,
    }

    impl VsyncClock {
        /// Made when the host's clock starts (`Desktop::start`), so the two share their zero.
        pub(super) fn new() -> Self {
            Self { origin_ns: monotonic_ns(), intervals: [16_666_667; 16], next: 0, last_vsync: 0, last_ms: 0.0 }
        }

        /// The time of a frame drawn for the vsync at `vsync_ns`: the next vsync, when it shows.
        /// Never before the frame before, nor far from the CPU's `now_ms`.
        pub(super) fn frame_ms(&mut self, vsync_ns: i64, now_ms: f64) -> f64 {
            let interval = vsync_ns - self.last_vsync;
            if self.last_vsync > 0 && (2_000_000..100_000_000).contains(&interval) {
                self.intervals[self.next] = interval;
                self.next = (self.next + 1) % self.intervals.len();
            }
            self.last_vsync = vsync_ns;
            let period = *self.intervals.iter().min().unwrap_or(&16_666_667);
            let ms = (vsync_ns + period - self.origin_ns) as f64 / 1_000_000.0;
            let ms = if (ms - now_ms).abs() > 250.0 { now_ms } else { ms };
            self.last_ms = ms.max(self.last_ms);
            self.last_ms
        }
    }

    /// A line in logcat, tag `drawnui`.
    pub(super) fn log(text: &str) {
        let (tag, text) = (c"drawnui", CString::new(text.replace('\0', " ")).unwrap_or_default());
        // SAFETY: two NUL-terminated strings that live through the call; 4 = ANDROID_LOG_INFO.
        unsafe { __android_log_write(4, tag.as_ptr(), text.as_ptr()) };
    }

    /// An Android system property (`adb shell setprop <name> <value>`); `None` when unset.
    pub(super) fn system_property(name: &str) -> Option<String> {
        unsafe extern "C" {
            fn __system_property_get(name: *const c_char, value: *mut c_char) -> c_int;
        }
        let name = CString::new(name).ok()?;
        let mut value = [0 as c_char; 92]; // PROP_VALUE_MAX
        // SAFETY: a NUL-terminated name and a buffer of PROP_VALUE_MAX bytes.
        let len = unsafe { __system_property_get(name.as_ptr(), value.as_mut_ptr()) };
        (len > 0).then(|| unsafe { std::ffi::CStr::from_ptr(value.as_ptr()) }.to_string_lossy().into_owned())
    }

    /// A panic message goes to logcat before the process ends.
    pub(super) fn log_panics() {
        std::panic::set_hook(Box::new(|info| log(&format!("panic: {info}"))));
    }

    /// `activity.moveTaskToBack(true)`: the app goes to the background, as Back does at an app's
    /// root since Android 12.
    pub(super) fn move_task_to_back() {
        use jni_sys::{JNIEnv, JavaVM, jobject, jvalue};
        use std::ptr::null_mut;
        let Some(app) = super::android_app() else { return };
        let (vm, activity) = (app.vm_as_ptr() as *mut JavaVM, app.activity_as_ptr() as jobject);
        // SAFETY: as `system_insets`: the activity's own method, from a thread attached to its VM.
        unsafe {
            let mut env: *mut JNIEnv = null_mut();
            let Some(attach) = (**vm).AttachCurrentThread else { return };
            attach(vm, &mut env as *mut *mut JNIEnv as *mut *mut std::ffi::c_void, null_mut());
            let e = &**env;
            let (Some(get_class), Some(get_method), Some(call), Some(delete)) = (e.GetObjectClass, e.GetMethodID, e.CallBooleanMethodA, e.DeleteLocalRef) else { return };
            let class = get_class(env, activity);
            let method = get_method(env, class, c"moveTaskToBack".as_ptr(), c"(Z)Z".as_ptr());
            delete(env, class);
            if !method.is_null() {
                call(env, activity, method, [jvalue { z: 1 }].as_ptr());
            }
            if let (Some(check), Some(clear)) = (e.ExceptionCheck, e.ExceptionClear)
                && check(env) != 0
            {
                clear(env);
            }
        }
    }

    /// The system bars and the display cutout over the window, pixels (left, top, right, bottom):
    /// `getWindow().getDecorView().getRootWindowInsets()`, its `getInsets(systemBars() |
    /// displayCutout())` from API 30, its system window insets before. `None` until the view has
    /// insets (not attached yet) or when a call throws.
    pub(super) fn system_insets() -> Option<[i32; 4]> {
        use jni_sys::{JNIEnv, JavaVM, jobject, jvalue};
        use std::ptr::{null, null_mut};
        let app = super::android_app()?;
        let (vm, activity) = (app.vm_as_ptr() as *mut JavaVM, app.activity_as_ptr() as jobject);
        // SAFETY: JNI calls on the activity the process runs, from a thread attached to its VM;
        // every class and method looked up exists at the API levels guarded below; local
        // references are freed by the VM when the thread returns to it (this one never does, so
        // they are deleted here).
        unsafe {
            let mut env: *mut JNIEnv = null_mut();
            ((**vm).AttachCurrentThread?)(vm, &mut env as *mut *mut JNIEnv as *mut *mut std::ffi::c_void, null_mut());
            let e = &**env;
            let failed = |env: *mut JNIEnv| {
                let thrown = (e.ExceptionCheck.unwrap())(env) != 0;
                if thrown {
                    (e.ExceptionClear.unwrap())(env);
                }
                thrown
            };
            let call_object = |obj: jobject, name: &std::ffi::CStr, signature: &std::ffi::CStr| -> Option<jobject> {
                let class = (e.GetObjectClass?)(env, obj);
                let method = (e.GetMethodID?)(env, class, name.as_ptr(), signature.as_ptr());
                (e.DeleteLocalRef?)(env, class);
                if method.is_null() || failed(env) {
                    return None;
                }
                let result = (e.CallObjectMethodA?)(env, obj, method, null());
                if failed(env) || result.is_null() { None } else { Some(result) }
            };
            let call_int = |obj: jobject, name: &std::ffi::CStr, signature: &std::ffi::CStr, args: &[jvalue]| -> Option<i32> {
                let class = (e.GetObjectClass?)(env, obj);
                let method = (e.GetMethodID?)(env, class, name.as_ptr(), signature.as_ptr());
                (e.DeleteLocalRef?)(env, class);
                if method.is_null() || failed(env) {
                    return None;
                }
                let value = (e.CallIntMethodA?)(env, obj, method, args.as_ptr());
                if failed(env) { None } else { Some(value) }
            };
            let window = call_object(activity, c"getWindow", c"()Landroid/view/Window;")?;
            let decor = call_object(window, c"getDecorView", c"()Landroid/view/View;");
            (e.DeleteLocalRef?)(env, window);
            let decor = decor?;
            let insets = call_object(decor, c"getRootWindowInsets", c"()Landroid/view/WindowInsets;");
            (e.DeleteLocalRef?)(env, decor);
            let insets = insets?;
            let result = if app.config().sdk_version() >= 30 {
                let types = (e.FindClass?)(env, c"android/view/WindowInsets$Type".as_ptr());
                let static_int = |name: &std::ffi::CStr| -> Option<i32> {
                    let method = (e.GetStaticMethodID?)(env, types, name.as_ptr(), c"()I".as_ptr());
                    if method.is_null() || failed(env) {
                        return None;
                    }
                    let value = (e.CallStaticIntMethodA?)(env, types, method, null());
                    if failed(env) { None } else { Some(value) }
                };
                let mask = static_int(c"systemBars").zip(static_int(c"displayCutout")).map(|(a, b)| a | b);
                (e.DeleteLocalRef?)(env, types);
                let rect = mask.and_then(|mask| {
                    let class = (e.GetObjectClass?)(env, insets);
                    let method = (e.GetMethodID?)(env, class, c"getInsets".as_ptr(), c"(I)Landroid/graphics/Insets;".as_ptr());
                    (e.DeleteLocalRef?)(env, class);
                    if method.is_null() || failed(env) {
                        return None;
                    }
                    let rect = (e.CallObjectMethodA?)(env, insets, method, [jvalue { i: mask }].as_ptr());
                    if failed(env) || rect.is_null() { None } else { Some(rect) }
                });
                rect.and_then(|rect| {
                    let class = (e.GetObjectClass?)(env, rect);
                    let field = |name: &std::ffi::CStr| -> Option<i32> {
                        let id = (e.GetFieldID?)(env, class, name.as_ptr(), c"I".as_ptr());
                        if id.is_null() || failed(env) {
                            return None;
                        }
                        Some((e.GetIntField?)(env, rect, id))
                    };
                    let sides = [field(c"left"), field(c"top"), field(c"right"), field(c"bottom")];
                    (e.DeleteLocalRef?)(env, class);
                    (e.DeleteLocalRef?)(env, rect);
                    Some([sides[0]?, sides[1]?, sides[2]?, sides[3]?])
                })
            } else {
                let side = |name: &std::ffi::CStr| call_int(insets, name, c"()I", &[]);
                Some([
                    side(c"getSystemWindowInsetLeft")?,
                    side(c"getSystemWindowInsetTop")?,
                    side(c"getSystemWindowInsetRight")?,
                    side(c"getSystemWindowInsetBottom")?,
                ])
            };
            (e.DeleteLocalRef?)(env, insets);
            result
        }
    }
}

/// Opens a link in the system browser, without a shell in between (a `&` in the link stays in it).
fn open_url(url: &str) {
    #[cfg(windows)]
    let mut command = std::process::Command::new("rundll32.exe");
    #[cfg(windows)]
    command.args(["url.dll,FileProtocolHandler", url]);
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "macos")]
    command.arg(url);
    #[cfg(not(any(windows, target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    #[cfg(not(any(windows, target_os = "macos")))]
    command.arg(url);
    if let Err(e) = command.spawn() {
        eprintln!("drawnui: could not open {url}: {e}");
    }
}

/// The system clipboard's text: Windows (Win32) and Linux (X11 selections); elsewhere nothing is
/// copied and a paste brings nothing.
mod clipboard {
    /// Linux: one clipboard for the app's life. An X11 selection is handed out by the app that
    /// copied it, so the clipboard stays open after a copy.
    #[cfg(target_os = "linux")]
    fn with<T>(f: impl FnOnce(&mut arboard::Clipboard) -> Option<T>) -> Option<T> {
        static CLIPBOARD: std::sync::Mutex<Option<arboard::Clipboard>> = std::sync::Mutex::new(None);
        let mut clipboard = CLIPBOARD.lock().ok()?;
        if clipboard.is_none() {
            *clipboard = arboard::Clipboard::new().ok();
        }
        f(clipboard.as_mut()?)
    }

    #[cfg(target_os = "linux")]
    pub fn set(text: &str) -> bool {
        with(|clipboard| clipboard.set_text(text).ok()).is_some()
    }

    #[cfg(target_os = "linux")]
    pub fn get() -> Option<String> {
        with(|clipboard| clipboard.get_text().ok())
    }

    #[cfg(windows)]
    pub fn set(text: &str) -> bool {
        const GMEM_MOVEABLE: u32 = 2;
        let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
        // SAFETY: the Win32 clipboard protocol: open, empty, hand over a moveable global block
        // (the system owns it once SetClipboardData succeeds), close.
        unsafe {
            if win::OpenClipboard(std::ptr::null_mut()) == 0 {
                return false;
            }
            win::EmptyClipboard();
            let memory = win::GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2);
            let mut done = false;
            if !memory.is_null() {
                let to = win::GlobalLock(memory) as *mut u16;
                if !to.is_null() {
                    std::ptr::copy_nonoverlapping(wide.as_ptr(), to, wide.len());
                    win::GlobalUnlock(memory);
                    done = !win::SetClipboardData(win::CF_UNICODETEXT, memory).is_null();
                }
                if !done {
                    win::GlobalFree(memory);
                }
            }
            win::CloseClipboard();
            done
        }
    }

    #[cfg(windows)]
    pub fn get() -> Option<String> {
        // SAFETY: the block belongs to the clipboard; it is read between lock and unlock, and
        // the clipboard stays open meanwhile.
        unsafe {
            if win::OpenClipboard(std::ptr::null_mut()) == 0 {
                return None;
            }
            let memory = win::GetClipboardData(win::CF_UNICODETEXT);
            let from = if memory.is_null() { std::ptr::null() } else { win::GlobalLock(memory) as *const u16 };
            let text = (!from.is_null()).then(|| {
                let len = (0..).take_while(|i| *from.add(*i) != 0).count();
                String::from_utf16_lossy(std::slice::from_raw_parts(from, len))
            });
            if !from.is_null() {
                win::GlobalUnlock(memory);
            }
            win::CloseClipboard();
            text
        }
    }

    #[cfg(windows)]
    #[allow(non_snake_case)]
    mod win {
        use std::ffi::c_void;
        pub const CF_UNICODETEXT: u32 = 13;
        #[link(name = "user32")]
        unsafe extern "system" {
            pub fn OpenClipboard(owner: *mut c_void) -> i32;
            pub fn CloseClipboard() -> i32;
            pub fn EmptyClipboard() -> i32;
            pub fn GetClipboardData(format: u32) -> *mut c_void;
            pub fn SetClipboardData(format: u32, memory: *mut c_void) -> *mut c_void;
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            pub fn GlobalAlloc(flags: u32, bytes: usize) -> *mut c_void;
            pub fn GlobalLock(memory: *mut c_void) -> *mut c_void;
            pub fn GlobalUnlock(memory: *mut c_void) -> i32;
            pub fn GlobalFree(memory: *mut c_void) -> *mut c_void;
        }
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    pub fn set(_text: &str) -> bool {
        false
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    pub fn get() -> Option<String> {
        None
    }
}

/// A key by its DOM `code` name: winit's `KeyCode` variants carry the same names, except the
/// Windows / Command keys (winit Super, DOM Meta).
macro_rules! desktop_key_name {
    ($($code:ident),*) => {
        fn key_name(code: KeyCode) -> InputKey {
            match code {
                $(KeyCode::$code => stringify!($code),)*
                KeyCode::SuperLeft => "MetaLeft",
                KeyCode::SuperRight => "MetaRight",
                _ => "Unknown",
            }
        }
    };
}
with_key_codes!(desktop_key_name);

fn mouse_button(button: WinitButton) -> Option<MouseButton> {
    match button {
        WinitButton::Left => Some(MouseButton::Left),
        WinitButton::Right => Some(MouseButton::Right),
        WinitButton::Middle => Some(MouseButton::Middle),
        WinitButton::Back => Some(MouseButton::Back),
        WinitButton::Forward => Some(MouseButton::Forward),
        WinitButton::Other(_) => None,
    }
}

impl ApplicationHandler<Arrived> for Desktop {
    /// The first one makes the screen-reader adapter and shows the window; later ones bring the
    /// app back to the foreground (iOS): frames again.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Android: AccessKit injects into GameActivity's surface view (TalkBack).
        if self.access.is_none() && self.window.is_visible() != Some(true) {
            self.access = Some(accesskit_winit::Adapter::with_event_loop_proxy(event_loop, &self.window, self.proxy.clone()));
        }
        self.window.set_visible(true);
        if self.suspended {
            // Android: a new native window came with the activity; its GL surface is made again.
            #[cfg(target_os = "android")]
            {
                self.presenter.resume(&self.window);
                self.surface = None;
                self.insets_read = false;
            }
            self.suspended = false;
            self.app.visibility(true);
            self.redraw();
        }
    }

    /// The app went to the background (iOS): no frame is submitted until `resumed`.
    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.suspended = true;
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        if let Some(ticker) = &self.ticker {
            ticker.run(false);
        }
        self.app.visibility(false);
        // Android: the native window goes away with the activity's surface.
        #[cfg(target_os = "android")]
        {
            self.surface = None;
            self.presenter.suspend();
        }
    }

    /// The time a timer asked for came.
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            // A paced frame that came due is drawn now, not held again.
            #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "android")))]
            if self.pacing.due.is_some_and(|at| at <= Instant::now()) {
                self.pacing.due = None;
                self.window.request_redraw();
                return;
            }
            self.redraw();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "android")]
        if self.ime_open {
            self.take_soft_keyboard_text();
        }
        let wake = self.wake;
        #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "android")))]
        let wake = match (wake, self.pacing.due) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        event_loop.set_control_flow(wake.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }

    /// A picture or a cache bitmap arrived from a worker.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, arrived: Arrived) {
        match arrived {
            Arrived::Image(id, decoded) => {
                let started = Instant::now();
                self.app.image(id, decoded);
                self.stats.image_ms.push(started.elapsed().as_secs_f32() * 1000.0);
            }
            Arrived::Baked(id, image) => self.app.baked(id, image),
            Arrived::Access(event) => self.access_event(event.window_event),
            // The display link ticked: the frame wanted is drawn; with none wanted it stops.
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            Arrived::Tick => {
                if !self.suspended && std::mem::take(&mut self.frame_wanted) {
                    self.draw();
                }
                if !self.frame_wanted
                    && let Some(ticker) = &self.ticker
                {
                    ticker.run(false);
                }
                return;
            }
            #[cfg(target_os = "android")]
            // Draws the frame wanted, and asks for the next callback before drawing, as C# keeps
            // its callback chain: a frame that ends after the next vsync still draws at the one
            // after it. With no frame wanted the chain stops; this event never asks for one.
            Arrived::Vsync(ns) => {
                self.frame_posted = false;
                if !self.suspended && std::mem::take(&mut self.frame_wanted) {
                    self.frame_posted = android::post_frame(&self.proxy);
                    self.vsync_ns = Some(ns);
                    self.window.request_redraw();
                }
                return;
            }
        }
        self.redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(access) = &mut self.access {
            access.process_event(&self.window, &event);
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                // The same size: only the safe area changed (Android hears insets as a resize),
                // the surface stays.
                #[cfg(target_os = "android")]
                let kept = std::mem::replace(&mut self.size, size) == size;
                #[cfg(not(target_os = "android"))]
                let kept = self.surface.as_ref().is_some_and(|s| (s.width() as u32, s.height() as u32) == (size.width, size.height));
                if !kept {
                    self.resized_at = Some(Instant::now());
                    self.surface = None;
                    self.presenter.resize(&self.window, size.width.max(1), size.height.max(1));
                }
                #[cfg(target_os = "android")]
                {
                    self.insets_read = false;
                }
                #[cfg(any(target_os = "ios", target_os = "android"))]
                self.report_safe_area();
                self.redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
                // No button down: a hover, which the app coalesces to one per frame.
                let (kind, button) = self.pressed.map_or((PointerKind::Hover, MouseButton::Left), |b| (PointerKind::Move, b));
                self.app.pointer_button(kind, button, self.cursor.0, self.cursor.1, self.now_ms());
                self.redraw();
            }
            WindowEvent::CursorLeft { .. } if self.pressed.is_none() => {
                self.app.pointer_button(PointerKind::Leave, MouseButton::Left, self.cursor.0, self.cursor.1, self.now_ms());
                self.redraw();
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(button) = mouse_button(button) else { return };
                let (x, y, now) = (self.cursor.0, self.cursor.1, self.now_ms());
                if state == ElementState::Pressed {
                    self.pressed.get_or_insert(button);
                    self.app.pointer_button(PointerKind::Down, button, x, y, now);
                } else {
                    if self.pressed == Some(button) {
                        self.pressed = None;
                    }
                    self.app.pointer_button(PointerKind::Up, button, x, y, now);
                    // Windows opens a context menu when the right button is released (the
                    // browsers' `contextmenu` event).
                    if button == MouseButton::Right {
                        self.app.context_menu(x, y, ContextMenuSource::Mouse, now);
                    }
                }
                self.redraw();
            }
            WindowEvent::Focused(false) => {
                // The window went to the background with a button down: its release will not arrive.
                if let Some(button) = self.pressed.take() {
                    self.app.pointer_button(PointerKind::Cancel, button, self.cursor.0, self.cursor.1, self.now_ms());
                }
                // Keys released elsewhere do not come back (React clears on `blur`).
                self.app.blur();
                self.redraw();
            }
            // Minimized or covered: an app pauses what it animates (a game).
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
                self.app.visibility(!occluded);
                self.redraw();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let m = modifiers.state();
                self.modifiers = Modifiers { shift: m.shift_key(), ctrl: m.control_key(), alt: m.alt_key(), meta: m.super_key() };
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // While an input method composes, Backspace, the arrows and Enter edit or pick its
                // text (Windows reports them as VK_PROCESSKEY, winit as `Process` with the physical
                // key): they never reach the app, or an editor would delete committed text.
                if self.ime_composing || event.logical_key == Key::Named(NamedKey::Process) {
                    return;
                }
                // Android's Back (button or gesture): the app's keys first, then the shell goes
                // back; with nothing to go back to, the app goes to the background as Android
                // apps do (winit reports every key handled, so the system would not).
                #[cfg(target_os = "android")]
                if event.logical_key == Key::Named(NamedKey::BrowserBack) {
                    let down = event.state == ElementState::Pressed;
                    let used = self.key(if down { KeyKind::Down } else { KeyKind::Up }, "BrowserBack", "", event.repeat);
                    if down && !event.repeat && !used && !self.app.back() {
                        android::move_task_to_back();
                    }
                    self.redraw();
                    return;
                }
                let key = match event.physical_key {
                    PhysicalKey::Code(code) => key_name(code),
                    // iOS's soft keyboard types Return as the text "\n" with no physical key: it is
                    // Enter for the app (a single-line editor submits).
                    #[cfg(target_os = "ios")]
                    PhysicalKey::Unidentified(_) if matches!(&event.logical_key, Key::Character(c) if c == "\n" || c == "\r") => "Enter",
                    PhysicalKey::Unidentified(_) => "Unknown",
                };
                let down = event.state == ElementState::Pressed;
                let used = self.key(if down { KeyKind::Down } else { KeyKind::Up }, key, "", event.repeat);
                // Typed text: printable, no Ctrl / Alt / Meta (React KeyChar); AltGr (Ctrl + Alt
                // on Windows) types characters too.
                let m = self.modifiers;
                let plain = !(m.ctrl || m.alt || m.meta) || (m.ctrl && m.alt && !m.meta);
                if let Some(text) = event.text.as_deref().filter(|t| down && plain && !t.chars().any(char::is_control)) {
                    self.key(KeyKind::Char, "", text, event.repeat);
                }
                // The Menu key asks for a context menu where the mouse is.
                if down && !used && key == "ContextMenu" {
                    let now = self.now_ms();
                    self.app.context_menu(self.cursor.0, self.cursor.1, ContextMenuSource::Keyboard, now);
                }
            }
            // Text an IME composed (Chinese, Japanese, the emoji panel).
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.ime_composing = false;
                self.key(KeyKind::Char, "", &text, false);
            }
            WindowEvent::Ime(Ime::Preedit(text, _)) => self.ime_composing = !text.is_empty(),
            WindowEvent::Ime(Ime::Disabled) => self.ime_composing = false,
            WindowEvent::MouseWheel { delta, .. } => {
                // Notches: a line is one; pixels (touchpads) count 100 points to a notch, as browsers do.
                let (x, y) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x, y),
                    MouseScrollDelta::PixelDelta(p) => {
                        let notch = 100.0 * self.window.scale_factor();
                        ((p.x / notch) as f32, (p.y / notch) as f32)
                    }
                };
                // The dominant axis, and which one it is: winit counts toward the start (up, left)
                // as positive on both.
                let (dominant, horizontal) = if y.abs() >= x.abs() { (y, false) } else { (x, true) };
                if self.app.wheel(self.cursor.0, self.cursor.1, dominant, horizontal, self.now_ms()) {
                    self.redraw();
                }
            }
            // One finger is the pointer, as the left button: no hover, no second finger yet (DrawnUI's
            // touch heads). A long press is the engine's LongPressing.
            #[cfg(any(target_os = "ios", target_os = "android"))]
            WindowEvent::Touch(touch) => {
                use winit::event::TouchPhase;
                let (x, y, now) = (touch.location.x as f32, touch.location.y as f32, self.now_ms());
                let kind = match touch.phase {
                    TouchPhase::Started if self.touch.is_none() => {
                        self.touch = Some(touch.id);
                        self.pressed = Some(MouseButton::Left);
                        PointerKind::Down
                    }
                    _ if self.touch != Some(touch.id) => return,
                    TouchPhase::Started | TouchPhase::Moved => PointerKind::Move,
                    TouchPhase::Ended => PointerKind::Up,
                    TouchPhase::Cancelled => PointerKind::Cancel,
                };
                if matches!(kind, PointerKind::Up | PointerKind::Cancel) {
                    (self.touch, self.pressed) = (None, None);
                }
                self.cursor = (x, y);
                self.app.pointer_touch(kind, x, y, now);
                self.redraw();
            }
            WindowEvent::RedrawRequested => self.draw(),
            _ => {}
        }
    }
}

/// Android: Vulkan (the default) or OpenGL ES, chosen when the window is made: the app's
/// `gpu_backend`, overridden by the system property `debug.drawnui.gpu` (`gl` / `vulkan`); OpenGL
/// ES also where Vulkan cannot be made on the window.
#[cfg(target_os = "android")]
mod android_gpu {
    use winit::event_loop::ActiveEventLoop;
    use winit::window::{Window, WindowAttributes};

    use super::{android, gl, vulkan};
    use crate::GpuBackend;
    use crate::{Gpu, skia};

    pub(super) enum Presenter {
        Gl(gl::Presenter),
        Vulkan(vulkan::Presenter),
    }

    impl Presenter {
        pub(super) fn new_active(event_loop: &ActiveEventLoop, attributes: WindowAttributes, backend: GpuBackend) -> (Window, Self) {
            let backend = match android::system_property("debug.drawnui.gpu").as_deref() {
                Some("gl" | "opengl") => GpuBackend::OpenGl,
                Some("vulkan" | "vk") => GpuBackend::Vulkan,
                _ => backend,
            };
            if backend == GpuBackend::OpenGl {
                android::log("drawnui: OpenGL ES");
                let (window, presenter) = gl::Presenter::new_active(event_loop, attributes);
                return (window, Self::Gl(presenter));
            }
            let window = event_loop.create_window(attributes).expect("no window");
            match vulkan::Presenter::new(&window) {
                Ok(presenter) => {
                    android::log(&format!("drawnui: Vulkan {}", presenter.describe()));
                    (window, Self::Vulkan(presenter))
                }
                Err(e) => {
                    android::log(&format!("drawnui: no Vulkan ({e}), OpenGL ES"));
                    let (window, presenter) = gl::Presenter::for_window(event_loop, window);
                    (window, Self::Gl(presenter))
                }
            }
        }

        pub(super) fn suspend(&mut self) {
            match self {
                Self::Gl(p) => p.suspend(),
                Self::Vulkan(p) => p.suspend(),
            }
        }

        pub(super) fn resume(&mut self, window: &Window) {
            match self {
                Self::Gl(p) => p.resume(window),
                Self::Vulkan(p) => p.resume(window),
            }
        }

        pub(super) fn gpu(&self) -> Gpu {
            match self {
                Self::Gl(p) => p.gpu(),
                Self::Vulkan(p) => p.gpu(None),
            }
        }

        pub(super) fn gpu_after(&mut self, window: &Window, lost: &mut Gpu) -> Gpu {
            match self {
                Self::Gl(p) => p.gpu_after(window, lost),
                Self::Vulkan(p) => p.gpu(Some(lost)),
            }
        }

        pub(super) fn begin<'a>(
            &mut self,
            gpu: &mut Gpu,
            surface: &'a mut Option<skia::Surface>,
            width: u32,
            height: u32,
        ) -> Option<&'a mut skia::Surface> {
            match self {
                Self::Gl(p) => p.begin(gpu, surface, width, height),
                Self::Vulkan(p) => p.begin(gpu, surface, width, height),
            }
        }

        pub(super) fn end_frame(&mut self, gpu: &mut Gpu, surface: &mut Option<skia::Surface>) {
            match self {
                Self::Gl(p) => p.end_frame(gpu, surface),
                Self::Vulkan(p) => p.end_frame(gpu, surface),
            }
        }

        pub(super) fn present(&mut self, surface: &mut Option<skia::Surface>) {
            match self {
                Self::Gl(p) => p.present(surface),
                Self::Vulkan(p) => p.present(surface),
            }
        }

        pub(super) fn resize(&mut self, window: &Window, width: u32, height: u32) {
            match self {
                Self::Gl(p) => p.resize(window, width, height),
                Self::Vulkan(p) => p.resize(width, height),
            }
        }

        /// The GPU ran everything submitted (before Skia frees what it used).
        pub(super) fn wait_idle(&mut self) {
            if let Self::Vulkan(p) = self {
                p.wait_idle();
            }
        }
    }
}

/// Android: Skia on Vulkan through ash. One instance and device for the activity; a swapchain on
/// its window (FIFO: presented at vsync, frames still asked for by Choreographer), made again when
/// the size changes or the window comes back. Frames are not waited for on the CPU: the GPU waits
/// for the image to be free (acquire semaphore) and presenting waits for the drawing (render
/// semaphore).
#[cfg(target_os = "android")]
mod vulkan {
    use std::ffi::{c_char, c_void};

    use ash::khr;
    use ash::vk::{self, Handle};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    use crate::{Gpu, skia};

    pub(super) struct Presenter {
        entry: ash::Entry,
        instance: ash::Instance,
        /// The API version the device is used at.
        version: u32,
        physical: vk::PhysicalDevice,
        device: ash::Device,
        family: u32,
        queue: vk::Queue,
        surface_fn: khr::surface::Instance,
        android_fn: khr::android_surface::Instance,
        swapchain_fn: khr::swapchain::Device,
        /// Null while the app is in the background (the native window is gone).
        surface: vk::SurfaceKHR,
        swapchain: vk::SwapchainKHR,
        rgba: bool,
        usage: vk::ImageUsageFlags,
        extent: vk::Extent2D,
        images: Vec<vk::Image>,
        /// Skia's surfaces on the swapchain images, made on first use; Skia follows their layout.
        surfaces: Vec<Option<skia::Surface>>,
        /// One more than the images: the GPU may still wait on the one before.
        acquire: Vec<vk::Semaphore>,
        /// One per image, signaled when its drawing ran.
        rendered: Vec<vk::Semaphore>,
        next: usize,
        /// The image of the frame being drawn, and whether its render semaphore will be signaled.
        frame: Option<(u32, bool)>,
        /// The swapchain must be made again (new size, a new window, out of date).
        stale: bool,
    }

    impl Presenter {
        pub(super) fn new(window: &Window) -> Result<Self, String> {
            // SAFETY: plain Vulkan setup; every object made here is destroyed on failure or in Drop.
            unsafe {
                let entry = ash::Entry::load().map_err(|e| format!("no libvulkan: {e}"))?;
                let loader = entry.try_enumerate_instance_version().ok().flatten().unwrap_or(vk::API_VERSION_1_0);
                let app = vk::ApplicationInfo::default().api_version(loader.min(vk::API_VERSION_1_3));
                let extensions = [khr::surface::NAME.as_ptr(), khr::android_surface::NAME.as_ptr()];
                let info = vk::InstanceCreateInfo::default().application_info(&app).enabled_extension_names(&extensions);
                let instance = entry.create_instance(&info, None).map_err(|e| format!("instance: {e}"))?;
                let surface_fn = khr::surface::Instance::new(&entry, &instance);
                let android_fn = khr::android_surface::Instance::new(&entry, &instance);
                let surface = match window_surface(&android_fn, window) {
                    Ok(surface) => surface,
                    Err(e) => {
                        instance.destroy_instance(None);
                        return Err(e);
                    }
                };
                // A device with a queue that draws and presents to the window.
                let found = instance.enumerate_physical_devices().unwrap_or_default().into_iter().find_map(|p| {
                    let families = instance.get_physical_device_queue_family_properties(p);
                    families.iter().enumerate().find_map(|(i, f)| {
                        let present = surface_fn.get_physical_device_surface_support(p, i as u32, surface).unwrap_or(false);
                        (f.queue_flags.contains(vk::QueueFlags::GRAPHICS) && present).then_some((p, i as u32))
                    })
                });
                let Some((physical, family)) = found else {
                    surface_fn.destroy_surface(surface, None);
                    instance.destroy_instance(None);
                    return Err("no device draws to the window".into());
                };
                let version = instance.get_physical_device_properties(physical).api_version.min(app.api_version);
                let priorities = [1.0];
                let queues = [vk::DeviceQueueCreateInfo::default().queue_family_index(family).queue_priorities(&priorities)];
                let extensions = [khr::swapchain::NAME.as_ptr()];
                let info = vk::DeviceCreateInfo::default().queue_create_infos(&queues).enabled_extension_names(&extensions);
                let device = match instance.create_device(physical, &info, None) {
                    Ok(device) => device,
                    Err(e) => {
                        surface_fn.destroy_surface(surface, None);
                        instance.destroy_instance(None);
                        return Err(format!("device: {e}"));
                    }
                };
                let queue = device.get_device_queue(family, 0);
                let swapchain_fn = khr::swapchain::Device::new(&instance, &device);
                Ok(Self {
                    entry,
                    instance,
                    version,
                    physical,
                    device,
                    family,
                    queue,
                    surface_fn,
                    android_fn,
                    swapchain_fn,
                    surface,
                    swapchain: vk::SwapchainKHR::null(),
                    rgba: true,
                    usage: vk::ImageUsageFlags::COLOR_ATTACHMENT,
                    extent: vk::Extent2D::default(),
                    images: Vec::new(),
                    surfaces: Vec::new(),
                    acquire: Vec::new(),
                    rendered: Vec::new(),
                    next: 0,
                    frame: None,
                    stale: true,
                })
            }
        }

        /// The device's name and API version, for the log.
        pub(super) fn describe(&self) -> String {
            // SAFETY: a physical device of the instance.
            let properties = unsafe { self.instance.get_physical_device_properties(self.physical) };
            let name = properties.device_name_as_c_str().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let v = self.version;
            format!("{}.{} on {name}", vk::api_version_major(v), vk::api_version_minor(v))
        }

        /// A Skia context on the device; after `lost`'s, the next epoch.
        pub(super) fn gpu(&self, lost: Option<&mut Gpu>) -> Gpu {
            let entry = self.entry.clone();
            let device_proc_addr = self.instance.fp_v1_0().get_device_proc_addr;
            let instance_proc = move |instance: u64, name: *const c_char| -> *const c_void {
                // SAFETY: a name Skia asks for, on the instance it was given.
                unsafe { entry.get_instance_proc_addr(vk::Instance::from_raw(instance), name) }.map_or(std::ptr::null(), |f| f as *const c_void)
            };
            let device_proc = move |device: u64, name: *const c_char| -> *const c_void {
                // SAFETY: as above, on the device.
                unsafe { device_proc_addr(vk::Device::from_raw(device), name) }.map_or(std::ptr::null(), |f| f as *const c_void)
            };
            // SAFETY: the handles live as long as this presenter, which the host drops after the context.
            // ponytail: a lost device is not made again; a new context on it fails (the host panics).
            unsafe {
                Gpu::new_vulkan(
                    self.instance.handle().as_raw(),
                    self.physical.as_raw(),
                    self.device.handle().as_raw(),
                    self.queue.as_raw(),
                    self.family,
                    self.version,
                    &instance_proc,
                    &device_proc,
                    lost,
                )
            }
            .expect("no Vulkan GPU context")
        }

        pub(super) fn suspend(&mut self) {
            self.wait_idle();
            self.destroy_swapchain();
            // SAFETY: nothing uses the surface any more.
            unsafe { self.surface_fn.destroy_surface(self.surface, None) };
            self.surface = vk::SurfaceKHR::null();
        }

        pub(super) fn resume(&mut self, window: &Window) {
            match window_surface(&self.android_fn, window) {
                Ok(surface) => self.surface = surface,
                Err(e) => super::android::log(&format!("drawnui: Vulkan: {e}")),
            }
            self.stale = true;
        }

        pub(super) fn resize(&mut self, width: u32, height: u32) {
            if (width, height) != (self.extent.width, self.extent.height) {
                self.stale = true;
            }
        }

        pub(super) fn wait_idle(&mut self) {
            // SAFETY: the device is alive.
            let _ = unsafe { self.device.device_wait_idle() };
        }

        /// The next swapchain image as the frame's surface; the GPU waits for it to be free.
        pub(super) fn begin<'a>(
            &mut self,
            gpu: &mut Gpu,
            surface: &'a mut Option<skia::Surface>,
            width: u32,
            height: u32,
        ) -> Option<&'a mut skia::Surface> {
            if self.surface.is_null() {
                return None;
            }
            if self.stale || (width, height) != (self.extent.width, self.extent.height) {
                self.make_swapchain(width, height)?;
            }
            let index = match self.acquire_image() {
                Some(index) => index,
                None => {
                    self.make_swapchain(width, height)?;
                    self.acquire_image()?
                }
            };
            let (w, h) = (self.extent.width as i32, self.extent.height as i32);
            let (raw, rgba, usage) = (self.images[index as usize].as_raw(), self.rgba, self.usage.as_raw());
            let slot = &mut self.surfaces[index as usize];
            if slot.is_none() {
                // SAFETY: an image of this swapchain, of `usage`; its surface goes before the swapchain.
                *slot = unsafe { gpu.vulkan_surface(raw, rgba, usage, w, h) };
            }
            let mut image = slot.clone()?;
            // SAFETY: the semaphore the acquire signals; unused again before this frame is presented.
            unsafe { gpu.vulkan_wait(&mut image, self.acquire[self.next].as_raw()) };
            self.frame = Some((index, false));
            *surface = Some(image);
            surface.as_mut()
        }

        /// The drawing submitted; it signals the image's render semaphore.
        pub(super) fn end_frame(&mut self, gpu: &mut Gpu, surface: &mut Option<skia::Surface>) {
            if let (Some(image), Some((index, _))) = (surface.as_mut(), self.frame) {
                // SAFETY: the image's render semaphore, waited on by its last present already.
                let signaled = unsafe { gpu.vulkan_flush(image, self.rendered[index as usize].as_raw()) };
                self.frame = Some((index, signaled));
            }
        }

        /// Presents the image once its drawing ran (FIFO: at a vsync).
        pub(super) fn present(&mut self, surface: &mut Option<skia::Surface>) {
            *surface = None;
            let Some((index, signaled)) = self.frame.take() else { return };
            let waits = [self.rendered[index as usize]];
            let (swapchains, indices) = ([self.swapchain], [index]);
            let mut info = vk::PresentInfoKHR::default().swapchains(&swapchains).image_indices(&indices);
            if signaled {
                info = info.wait_semaphores(&waits);
            }
            // SAFETY: an image acquired for this frame, its drawing submitted.
            match unsafe { self.swapchain_fn.queue_present(self.queue, &info) } {
                Ok(suboptimal) => self.stale |= suboptimal,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => self.stale = true,
                Err(e) => super::android::log(&format!("drawnui: Vulkan present: {e}")),
            }
            self.next = (self.next + 1) % self.acquire.len().max(1);
        }

        fn acquire_image(&mut self) -> Option<u32> {
            // SAFETY: the swapchain is alive; the semaphore is not waited on by a pending frame.
            match unsafe { self.swapchain_fn.acquire_next_image(self.swapchain, u64::MAX, self.acquire[self.next], vk::Fence::null()) } {
                Ok((index, suboptimal)) => {
                    self.stale |= suboptimal;
                    Some(index)
                }
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => None,
                Err(e) => {
                    super::android::log(&format!("drawnui: Vulkan acquire: {e}"));
                    None
                }
            }
        }

        /// A swapchain for the window's size: RGBA8 (else BGRA8), FIFO, three images where
        /// allowed, usable as Skia's render target and copy source (SkiaBackdrop).
        fn make_swapchain(&mut self, width: u32, height: u32) -> Option<()> {
            self.wait_idle();
            self.surfaces.clear();
            // SAFETY: the device is idle; the old swapchain goes after the new one is made from it.
            unsafe {
                let caps = self.surface_fn.get_physical_device_surface_capabilities(self.physical, self.surface).ok()?;
                let formats = self.surface_fn.get_physical_device_surface_formats(self.physical, self.surface).ok()?;
                let format = [vk::Format::R8G8B8A8_UNORM, vk::Format::B8G8R8A8_UNORM]
                    .into_iter()
                    .find_map(|wanted| formats.iter().find(|f| f.format == wanted))
                    .copied()?;
                let extent = if caps.current_extent.width != u32::MAX {
                    caps.current_extent
                } else {
                    vk::Extent2D {
                        width: width.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
                        height: height.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
                    }
                };
                let mut count = caps.min_image_count.max(3);
                if caps.max_image_count > 0 {
                    count = count.min(caps.max_image_count);
                }
                let optional = vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED;
                let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | (caps.supported_usage_flags & optional);
                let transform = if caps.supported_transforms.contains(vk::SurfaceTransformFlagsKHR::IDENTITY) {
                    vk::SurfaceTransformFlagsKHR::IDENTITY
                } else {
                    caps.current_transform
                };
                let alpha = [
                    vk::CompositeAlphaFlagsKHR::OPAQUE,
                    vk::CompositeAlphaFlagsKHR::INHERIT,
                    vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
                    vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
                ]
                .into_iter()
                .find(|a| caps.supported_composite_alpha.contains(*a))
                .unwrap_or(vk::CompositeAlphaFlagsKHR::OPAQUE);
                let old = self.swapchain;
                let info = vk::SwapchainCreateInfoKHR::default()
                    .surface(self.surface)
                    .min_image_count(count)
                    .image_format(format.format)
                    .image_color_space(format.color_space)
                    .image_extent(extent)
                    .image_array_layers(1)
                    .image_usage(usage)
                    .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                    .pre_transform(transform)
                    .composite_alpha(alpha)
                    .present_mode(vk::PresentModeKHR::FIFO)
                    .clipped(true)
                    .old_swapchain(old);
                let made = self.swapchain_fn.create_swapchain(&info, None);
                self.destroy_swapchain();
                let swapchain = match made {
                    Ok(swapchain) => swapchain,
                    Err(e) => {
                        super::android::log(&format!("drawnui: Vulkan swapchain: {e}"));
                        return None;
                    }
                };
                self.swapchain = swapchain;
                self.images = self.swapchain_fn.get_swapchain_images(swapchain).ok()?;
                self.surfaces = self.images.iter().map(|_| None).collect();
                let device = &self.device;
                let semaphore = || device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None).ok();
                self.acquire = (0..=self.images.len()).filter_map(|_| semaphore()).collect();
                self.rendered = (0..self.images.len()).filter_map(|_| semaphore()).collect();
                self.rgba = format.format == vk::Format::R8G8B8A8_UNORM;
                (self.usage, self.extent, self.next, self.stale) = (usage, extent, 0, false);
            }
            Some(())
        }

        /// The swapchain, its surfaces and semaphores go (the device is idle).
        fn destroy_swapchain(&mut self) {
            self.surfaces.clear();
            self.images.clear();
            // SAFETY: nothing on the GPU uses them any more.
            unsafe {
                for semaphore in self.acquire.drain(..).chain(self.rendered.drain(..)) {
                    self.device.destroy_semaphore(semaphore, None);
                }
                if !self.swapchain.is_null() {
                    self.swapchain_fn.destroy_swapchain(self.swapchain, None);
                }
            }
            self.swapchain = vk::SwapchainKHR::null();
        }
    }

    impl Drop for Presenter {
        fn drop(&mut self) {
            self.wait_idle();
            self.destroy_swapchain();
            // SAFETY: the device is idle and the Skia context that used it is gone.
            unsafe {
                if !self.surface.is_null() {
                    self.surface_fn.destroy_surface(self.surface, None);
                }
                self.device.destroy_device(None);
                self.instance.destroy_instance(None);
            }
        }
    }

    /// A Vulkan surface on the activity's native window.
    fn window_surface(android_fn: &khr::android_surface::Instance, window: &Window) -> Result<vk::SurfaceKHR, String> {
        let handle = window.window_handle().map_err(|e| e.to_string())?.as_raw();
        let RawWindowHandle::AndroidNdk(handle) = handle else { return Err("not an Android window".into()) };
        let info = vk::AndroidSurfaceCreateInfoKHR::default().window(handle.a_native_window.as_ptr().cast());
        // SAFETY: the activity's live native window.
        unsafe { android_fn.create_android_surface(&info, None) }.map_err(|e| format!("surface: {e}"))
    }
}

/// Windows and Linux: an OpenGL context through glutin, Skia on its default framebuffer.
#[cfg(not(any(target_os = "macos", target_os = "ios")))]
mod gl {
    use std::{ffi::CString, num::NonZeroU32};

    use glutin::{
        config::{Config as GlConfigHandle, ConfigTemplateBuilder, GlConfig},
        context::{ContextApi, ContextAttributesBuilder, NotCurrentContext, PossiblyCurrentContext, Robustness},
        display::{GetGlDisplay, GlDisplay},
        prelude::{GlSurface, NotCurrentGlContext},
        surface::{Surface as GlSurfaceHandle, SurfaceAttributesBuilder, SwapInterval, WindowSurface},
    };
    use glutin_winit::DisplayBuilder;
    use raw_window_handle::HasWindowHandle;
    #[cfg(not(target_os = "android"))]
    use winit::event_loop::EventLoop;
    use winit::window::{Window, WindowAttributes};

    #[cfg(not(target_os = "android"))]
    use super::Arrived;
    use crate::{Gpu, skia};

    /// The window and the GL config with the fewest samples (Skia does its own anti-aliasing), from
    /// either of winit's event loops: glutin-winit's trait for them is private.
    macro_rules! display {
        ($event_loop:expr, $attributes:expr) => {
            DisplayBuilder::new()
                .with_window_attributes($attributes.into())
                .build($event_loop, ConfigTemplateBuilder::new().with_alpha_size(8).with_stencil_size(8), |configs| {
                    configs.reduce(|best, c| if c.num_samples() < best.num_samples() { c } else { best }).unwrap()
                })
                .expect("no GL config")
        };
    }

    pub(super) struct Presenter {
        /// `None` while the app is in the background (Android): the native window is gone.
        gl_surface: Option<GlSurfaceHandle<WindowSurface>>,
        gl_context: PossiblyCurrentContext,
        /// Makes a new context when the GPU lost the old one.
        gl_config: GlConfigHandle,
        samples: usize,
        stencil: usize,
    }

    impl Presenter {
        #[cfg(not(target_os = "android"))]
        pub(super) fn new(event_loop: &EventLoop<Arrived>, attributes: WindowAttributes) -> (Window, Self) {
            let (window, gl_config) = display!(event_loop, attributes);
            Self::with(window, gl_config)
        }

        /// `new` from a running event loop (Android makes its window on the first `resumed`).
        #[cfg(target_os = "android")]
        pub(super) fn new_active(event_loop: &winit::event_loop::ActiveEventLoop, attributes: WindowAttributes) -> (Window, Self) {
            let (window, gl_config) = display!(event_loop, attributes);
            Self::with(window, gl_config)
        }

        /// `new_active` on a window made already (Android, after Vulkan could not be made on it).
        #[cfg(target_os = "android")]
        pub(super) fn for_window(event_loop: &winit::event_loop::ActiveEventLoop, window: Window) -> (Window, Self) {
            let (_, gl_config) = DisplayBuilder::new()
                .build(event_loop, ConfigTemplateBuilder::new().with_alpha_size(8).with_stencil_size(8), |configs| {
                    configs.reduce(|best, c| if c.num_samples() < best.num_samples() { c } else { best }).unwrap()
                })
                .expect("no GL config");
            Self::with(Some(window), gl_config)
        }

        fn with(window: Option<Window>, gl_config: GlConfigHandle) -> (Window, Self) {
            let window = window.expect("no window");
            let raw_handle = window.window_handle().expect("no window handle").as_raw();
            let context = gl_context_for(&gl_config, raw_handle);

            let gl_surface = window_surface(&gl_config, &window);
            let gl_context = context.make_current(&gl_surface).expect("GL context not current");
            if let Err(e) = gl_surface.set_swap_interval(&gl_context, SwapInterval::Wait(NonZeroU32::new(1).unwrap())) {
                eprintln!("drawnui: vsync not set: {e}");
            }
            let samples = gl_config.num_samples() as usize;
            let stencil = gl_config.stencil_size() as usize;
            (window, Self { gl_surface: Some(gl_surface), gl_context, gl_config, samples, stencil })
        }

        /// The native window went away (Android, the app in the background): its surface goes.
        #[cfg(target_os = "android")]
        pub(super) fn suspend(&mut self) {
            self.gl_surface = None;
        }

        /// A new native window (Android, back in the foreground): a surface on it, current.
        #[cfg(target_os = "android")]
        pub(super) fn resume(&mut self, window: &Window) {
            use glutin::prelude::PossiblyCurrentGlContext;
            let gl_surface = window_surface(&self.gl_config, window);
            self.gl_context.make_current(&gl_surface).expect("GL context not current");
            self.gl_surface = Some(gl_surface);
        }

        pub(super) fn gpu(&self) -> Gpu {
            Gpu::new_gl(gl_interface(&self.gl_config)).expect("no GPU context")
        }

        /// A new GL context and Skia context on the same window after the GPU lost the old ones.
        pub(super) fn gpu_after(&mut self, window: &Window, lost: &mut Gpu) -> Gpu {
            let raw_handle = window.window_handle().expect("no window handle").as_raw();
            let surface = self.gl_surface.as_ref().expect("a window surface");
            self.gl_context = gl_context_for(&self.gl_config, raw_handle).make_current(surface).expect("GL context not current");
            Gpu::new_gl_after(gl_interface(&self.gl_config), lost).expect("no GPU context")
        }

        /// The window's surface: the default framebuffer, wrapped once per size.
        pub(super) fn begin<'a>(
            &mut self,
            gpu: &mut Gpu,
            surface: &'a mut Option<skia::Surface>,
            width: u32,
            height: u32,
        ) -> Option<&'a mut skia::Surface> {
            if surface.is_none() {
                *surface = Some(gpu.window_surface(width as i32, height as i32, self.samples, self.stencil).expect("no window surface"));
            }
            surface.as_mut()
        }

        /// The frame's GPU work submitted.
        pub(super) fn end_frame(&mut self, gpu: &mut Gpu, surface: &mut Option<skia::Surface>) {
            if let Some(surface) = surface {
                gpu.end_frame(surface);
            }
        }

        pub(super) fn present(&mut self, _surface: &mut Option<skia::Surface>) {
            if let Some(gl_surface) = &self.gl_surface {
                gl_surface.swap_buffers(&self.gl_context).expect("swap failed");
            }
        }

        pub(super) fn resize(&mut self, _window: &Window, width: u32, height: u32) {
            if let Some(gl_surface) = &self.gl_surface {
                gl_surface.resize(&self.gl_context, NonZeroU32::new(width).unwrap(), NonZeroU32::new(height).unwrap());
            }
        }
    }

    /// A GL surface on the window, its size.
    fn window_surface(gl_config: &GlConfigHandle, window: &Window) -> GlSurfaceHandle<WindowSurface> {
        let raw_handle = window.window_handle().expect("no window handle").as_raw();
        let (width, height): (u32, u32) = window.inner_size().into();
        let surface_attributes = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            raw_handle,
            NonZeroU32::new(width.max(1)).unwrap(),
            NonZeroU32::new(height.max(1)).unwrap(),
        );
        // SAFETY: the window outlives the surface (field order of `Desktop`; Android drops it on suspend).
        unsafe { gl_config.display().create_window_surface(gl_config, &surface_attributes).expect("no GL surface") }
    }

    /// A GL context for the window. It asks to be told of a GPU reset (the context is then lost and
    /// made again, see `Desktop::recreate_gpu`); a driver without that gives a plain one, then GLES.
    fn gl_context_for(gl_config: &GlConfigHandle, raw_handle: raw_window_handle::RawWindowHandle) -> NotCurrentContext {
        let display = gl_config.display();
        let robust = ContextAttributesBuilder::new().with_robustness(Robustness::RobustLoseContextOnReset).build(Some(raw_handle));
        let plain = ContextAttributesBuilder::new().build(Some(raw_handle));
        let gles = ContextAttributesBuilder::new().with_context_api(ContextApi::Gles(None)).build(Some(raw_handle));
        // SAFETY: the window behind `raw_handle` outlives the context (field order of `Desktop`).
        unsafe {
            display
                .create_context(gl_config, &robust)
                .or_else(|_| display.create_context(gl_config, &plain))
                .or_else(|_| display.create_context(gl_config, &gles))
                .expect("no GL context")
        }
    }

    /// Skia's GL functions, from the current context.
    fn gl_interface(gl_config: &GlConfigHandle) -> skia::gpu::gl::Interface {
        let display = gl_config.display();
        skia::gpu::gl::Interface::new_load_with(|name| {
            if name == "eglGetCurrentDisplay" {
                return std::ptr::null();
            }
            display.get_proc_address(CString::new(name).unwrap().as_c_str())
        })
        .expect("no GL interface")
    }
}

/// macOS and iOS: Metal. A CAMetalLayer on the window's view (macOS: the view's layer; iOS: a
/// sublayer the size of the view); every frame draws into the layer's next drawable, and the layer
/// paces the frames to the display. Layer, drawable and touch share one scale: winit's, which is
/// `UIScreen.scale` on iOS (DrawnUI uses it, not `nativeScale`, which Display Zoom changes).
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod metal {
    use objc2::{
        rc::{Retained, autoreleasepool},
        runtime::ProtocolObject,
    };
    #[cfg(target_os = "macos")]
    use objc2_app_kit::NSView;
    use objc2_core_foundation::CGSize;
    #[cfg(target_os = "ios")]
    use objc2_ui_kit::UIView;
    use objc2_metal::{MTLCommandBuffer, MTLCommandQueue, MTLCreateSystemDefaultDevice, MTLDevice, MTLDrawable, MTLPixelFormat};
    use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    use crate::{Gpu, skia, skia::gpu::mtl};

    // Field order is drop order: the frame's drawable, then the layer, the queue, the device.
    pub(super) struct Presenter {
        /// The drawable of the frame being drawn, between `begin` and `present`.
        drawable: Option<Retained<ProtocolObject<dyn CAMetalDrawable>>>,
        layer: Retained<CAMetalLayer>,
        queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
        device: Retained<ProtocolObject<dyn MTLDevice>>,
        /// The view the layer sits in: its bounds give the layer's frame (iOS).
        #[cfg(target_os = "ios")]
        view: Retained<UIView>,
    }

    impl Presenter {
        /// `background`: the app's canvas background (`App::background`); iOS paints the view and
        /// the window behind the canvas with it.
        pub(super) fn new(window: Window, background: skia::Color) -> (Window, Self) {
            #[cfg(not(target_os = "ios"))]
            let _ = background;
            let device = MTLCreateSystemDefaultDevice().expect("no Metal device");
            let queue = device.newCommandQueue().expect("no Metal command queue");
            let layer = CAMetalLayer::new();
            layer.setDevice(Some(&device));
            layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
            // `Gpu::snapshot_canvas` reads the window surface (backdrop and blur effects).
            layer.setFramebufferOnly(false);
            layer.setPresentsWithTransaction(false);
            #[cfg(target_os = "macos")]
            let presenter = {
                let RawWindowHandle::AppKit(handle) = window.window_handle().expect("no window handle").as_raw() else {
                    panic!("not an AppKit window");
                };
                // SAFETY: winit's view lives as long as the window, which outlives the presenter.
                let view = unsafe { &*(handle.ns_view.as_ptr() as *const NSView) };
                view.setWantsLayer(true);
                view.setLayer(Some(&layer.clone().into_super()));
                Self { drawable: None, layer, queue, device }
            };
            #[cfg(target_os = "ios")]
            let presenter = {
                let RawWindowHandle::UiKit(handle) = window.window_handle().expect("no window handle").as_raw() else {
                    panic!("not a UIKit window");
                };
                // SAFETY: winit's view is a live UIView; retained here, it outlives the layer.
                let view = unsafe { Retained::retain(handle.ui_view.as_ptr() as *mut UIView) }.expect("no view");
                view.layer().addSublayer(&layer);
                // The canvas background under the first frame, as the launch screen: no flash of
                // another color at start (a transparent canvas keeps UIKit's own).
                if background.a() > 0 {
                    let f = |c: u8| c as f64 / 255.0;
                    let color = objc2_ui_kit::UIColor::colorWithRed_green_blue_alpha(
                        f(background.r()),
                        f(background.g()),
                        f(background.b()),
                        f(background.a()),
                    );
                    view.setBackgroundColor(Some(&color));
                    if let Some(window) = view.window() {
                        window.setBackgroundColor(Some(&color));
                    }
                }
                Self { drawable: None, layer, queue, device, view }
            };
            #[cfg(target_os = "ios")]
            let size = window.outer_size();
            #[cfg(not(target_os = "ios"))]
            let size = window.inner_size();
            presenter.resize(&window, size.width.max(1), size.height.max(1));
            (window, presenter)
        }

        fn handles(&self) -> (mtl::Handle, mtl::Handle) {
            (Retained::as_ptr(&self.device) as mtl::Handle, Retained::as_ptr(&self.queue) as mtl::Handle)
        }

        pub(super) fn gpu(&self) -> Gpu {
            let (device, queue) = self.handles();
            // SAFETY: the device and the queue live in the presenter, which outlives the context.
            unsafe { Gpu::new_metal(device, queue) }.expect("no GPU context")
        }

        pub(super) fn gpu_after(&mut self, _window: &Window, lost: &mut Gpu) -> Gpu {
            let (device, queue) = self.handles();
            // SAFETY: as in `gpu`.
            unsafe { Gpu::new_metal_after(device, queue, lost) }.expect("no GPU context")
        }

        /// The frame's surface on the layer's next drawable. `None` when the layer gives none.
        pub(super) fn begin<'a>(
            &mut self,
            gpu: &mut Gpu,
            surface: &'a mut Option<skia::Surface>,
            _width: u32,
            _height: u32,
        ) -> Option<&'a mut skia::Surface> {
            let drawable = self.layer.nextDrawable()?;
            let size = self.layer.drawableSize();
            let texture = drawable.texture();
            // SAFETY: the texture belongs to the drawable, kept until `present` drops the surface.
            *surface = unsafe { gpu.drawable_surface(Retained::as_ptr(&texture) as mtl::Handle, size.width as i32, size.height as i32) };
            self.drawable = Some(drawable);
            surface.as_mut()
        }

        /// The frame's GPU work submitted.
        pub(super) fn end_frame(&mut self, gpu: &mut Gpu, surface: &mut Option<skia::Surface>) {
            if let Some(surface) = surface {
                gpu.end_frame(surface);
            }
        }

        /// Presents the drawable after the frame's GPU work (`Gpu::end_frame` submitted it).
        pub(super) fn present(&mut self, surface: &mut Option<skia::Surface>) {
            *surface = None;
            let Some(drawable) = self.drawable.take() else { return };
            autoreleasepool(|_| {
                let Some(commands) = self.queue.commandBuffer() else { return };
                let drawable: Retained<ProtocolObject<dyn MTLDrawable>> = (&drawable).into();
                commands.presentDrawable(&drawable);
                commands.commit();
            });
        }

        /// The layer follows the window: its size in pixels and its scale (Retina).
        pub(super) fn resize(&self, window: &Window, width: u32, height: u32) {
            #[cfg(target_os = "ios")]
            self.layer.setFrame(self.view.bounds());
            self.layer.setContentsScale(window.scale_factor());
            self.layer.setDrawableSize(CGSize::new(width as f64, height as f64));
        }
    }
}

/// iOS: the window and everything drawn with it come with the first `resumed` (UIKit makes
/// windows only once the app has launched); events go to the `Desktop` from then on.
#[cfg(any(target_os = "ios", target_os = "android"))]
mod ios {
    use winit::{
        application::ApplicationHandler,
        event::{StartCause, WindowEvent},
        event_loop::{ActiveEventLoop, EventLoopProxy},
        window::{WindowAttributes, WindowId},
    };

    use super::{Arrived, Desktop, Presenter};

    use crate::App;

    pub(super) struct Launch {
        pub attributes: Option<WindowAttributes>,
        pub make: fn() -> Box<dyn App>,
        pub proxy: EventLoopProxy<Arrived>,
        pub desktop: Option<Desktop>,
    }

    impl ApplicationHandler<Arrived> for Launch {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            match (&mut self.desktop, self.attributes.take()) {
                (Some(desktop), _) => desktop.resumed(event_loop),
                (None, Some(attributes)) => {
                    let app = (self.make)();
                    #[cfg(target_os = "ios")]
                    let (window, presenter) = Presenter::new(event_loop.create_window(attributes).expect("no window"), app.background());
                    #[cfg(target_os = "android")]
                    let (window, presenter) = Presenter::new_active(event_loop, attributes, app.gpu_backend());
                    self.desktop = Some(Desktop::new(window, presenter, app, self.proxy.clone()));
                    // The first resume reaches the host too: it makes the screen-reader adapter
                    // (TalkBack; VoiceOver through accesskit_ios, which subclasses winit's view).
                    if let Some(desktop) = &mut self.desktop {
                        desktop.resumed(event_loop);
                    }
                }
                (None, None) => {}
            }
        }

        fn suspended(&mut self, event_loop: &ActiveEventLoop) {
            if let Some(desktop) = &mut self.desktop {
                desktop.suspended(event_loop);
            }
        }

        fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
            if let Some(desktop) = &mut self.desktop {
                desktop.new_events(event_loop, cause);
            }
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            if let Some(desktop) = &mut self.desktop {
                desktop.about_to_wait(event_loop);
            }
        }

        fn user_event(&mut self, event_loop: &ActiveEventLoop, arrived: Arrived) {
            if let Some(desktop) = &mut self.desktop {
                desktop.user_event(event_loop, arrived);
            }
        }

        fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
            if let Some(desktop) = &mut self.desktop {
                desktop.window_event(event_loop, id, event);
            }
        }
    }
}

/// Windows, Linux (OpenGL): the host asks the driver to wait for vsync at every swap, but a driver
/// may ignore it (WSLg's software OpenGL returned from every swap at once: 600 frames a second,
/// shown at WSLg's own moments, a ball that stutters). The intervals of the first 60 frames drawn
/// in a row show it; then frames are spaced one refresh apart by the host, as DrawnUI's OpenTK
/// host does with vsync off (`Super.MaxFps` = the monitor's refresh). Where vsync holds nothing
/// changes.
#[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "android")))]
struct Pacing {
    /// The monitor's refresh period.
    period: Duration,
    /// Off until the driver is seen ignoring vsync; `None` while that is not known yet.
    on: Option<bool>,
    intervals: Vec<f32>,
    last_start: Option<Instant>,
    /// When pacing: the last frame's slot on the refresh grid. The next slot is one period later,
    /// whenever the wake-up came, so late wake-ups do not add up (16.89 ms instead of 16.67).
    slot: Option<Instant>,
    /// The time the held frame may be drawn.
    due: Option<Instant>,
}

#[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "android")))]
impl Pacing {
    fn new(window: &Window) -> Self {
        let millihertz = window.current_monitor().and_then(|monitor| monitor.refresh_rate_millihertz()).filter(|&mhz| mhz >= 20_000);
        let period = Duration::from_secs_f64(1000.0 / millihertz.unwrap_or(60_000) as f64);
        Self { period, on: None, intervals: Vec::with_capacity(60), last_start: None, slot: None, due: None }
    }

    /// A frame starts: while vsync is not judged yet, its interval counts (frames in a row only).
    /// When pacing, the frame's slot: the one it was held for, else (after a pause, or far behind)
    /// its start.
    fn frame(&mut self, start: Instant) -> Option<Instant> {
        let previous = self.last_start.replace(start);
        if self.on == Some(true) {
            let next = self.slot.map(|slot| slot + self.period);
            let slot = match next {
                Some(next) if start >= next && start - next < self.period => next,
                _ => start,
            };
            self.slot = Some(slot);
            return Some(slot);
        }
        if self.on.is_some() {
            return None;
        }
        let interval = (start - previous?).as_secs_f32() * 1000.0;
        if interval > 100.0 {
            self.intervals.clear();
            return None;
        }
        self.intervals.push(interval);
        if self.intervals.len() == 60 {
            self.intervals.sort_by(f32::total_cmp);
            let (median, period_ms) = (self.intervals[30], self.period.as_secs_f32() * 1000.0);
            let ignored = median < period_ms / 2.0;
            if ignored {
                eprintln!("drawnui: the driver does not wait for vsync (frames {median:.1} ms apart): frames paced at {:.0} Hz", 1000.0 / period_ms);
            }
            self.on = Some(ignored);
        }
        None
    }

    /// When pacing: the time a frame asked for now may start, if that is later than now.
    fn hold(&self, now: Instant) -> Option<Instant> {
        if self.on != Some(true) {
            return None;
        }
        let at = self.slot.or(self.last_start)? + self.period;
        (at > now).then_some(at)
    }
}

/// The display link that paces frames on Apple platforms (C# Super.iOS / Super.Mac
/// `CADisplayLink`): paused while no frame is wanted, its rate the display's (120 Hz on ProMotion;
/// on iOS with `CADisableMinimumFrameDurationOnPhone` in Info.plist) or `crate::max_fps` snapped
/// to a divisor of it, as `Super.MaxFps`. iOS: `CADisplayLink`. macOS 14+: the window view's
/// `displayLink`, which follows the view to whatever screen it is on; below 14 there is none
/// (`new` gives `None`) and frames stay winit's redraws.
#[cfg(any(target_os = "macos", target_os = "ios"))]
mod ticker {
    use std::cell::Cell;

    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, sel};
    #[cfg(target_os = "macos")]
    use objc2_app_kit::NSView;
    #[cfg(target_os = "macos")]
    use objc2_foundation::NSObjectProtocol;
    use objc2_foundation::{NSObject, NSRunLoop, NSRunLoopCommonModes};
    use objc2_quartz_core::{CADisplayLink, CAFrameRateRange};
    #[cfg(target_os = "ios")]
    use objc2_ui_kit::UIScreen;
    use winit::{event_loop::EventLoopProxy, window::Window};

    use super::Arrived;

    pub(crate) struct Target {
        proxy: EventLoopProxy<Arrived>,
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "DrawnUiTicker"]
        #[ivars = Target]
        pub(crate) struct TickTarget;

        impl TickTarget {
            #[unsafe(method(tick:))]
            fn tick(&self, _link: &CADisplayLink) {
                let _ = self.ivars().proxy.send_event(Arrived::Tick);
            }
        }
    );

    pub(crate) struct Ticker {
        link: Retained<CADisplayLink>,
        _target: Retained<TickTarget>,
        /// The view whose screen gives the display's rate (macOS: it changes with the screen).
        #[cfg(target_os = "macos")]
        view: Retained<NSView>,
        /// The display's highest rate (iOS: one screen).
        #[cfg(target_os = "ios")]
        display_fps: u32,
        /// The rate the link was last given.
        applied: Cell<u32>,
    }

    impl Ticker {
        pub fn new(proxy: EventLoopProxy<Arrived>, window: &Window) -> Option<Self> {
            let mtm = MainThreadMarker::new().expect("not on the main thread");
            let target = mtm.alloc::<TickTarget>().set_ivars(Target { proxy });
            let target: Retained<TickTarget> = unsafe { msg_send![super(target), init] };
            #[cfg(target_os = "ios")]
            let _ = window;
            // SAFETY: the target answers `tick:` with the link as its argument.
            #[cfg(target_os = "ios")]
            let link = unsafe { CADisplayLink::displayLinkWithTarget_selector(&target, sel!(tick:)) };
            #[cfg(target_os = "macos")]
            let (link, view) = {
                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                let RawWindowHandle::AppKit(handle) = window.window_handle().ok()?.as_raw() else { return None };
                // SAFETY: winit's view is a live NSView; retained here, it outlives the link.
                let view = unsafe { Retained::retain(handle.ns_view.as_ptr() as *mut NSView) }?;
                // NSView.displayLink is macOS 14+.
                if !view.respondsToSelector(sel!(displayLinkWithTarget:selector:)) {
                    return None;
                }
                // SAFETY: as above.
                (unsafe { view.displayLinkWithTarget_selector(&target, sel!(tick:)) }, view)
            };
            link.setPaused(true);
            // SAFETY: the main run loop, on the main thread; common modes keep it ticking while a
            // touch is tracked (iOS) or the window is resized (macOS).
            unsafe { link.addToRunLoop_forMode(&NSRunLoop::mainRunLoop(), NSRunLoopCommonModes) };
            Some(Self {
                link,
                _target: target,
                #[cfg(target_os = "macos")]
                view,
                #[cfg(target_os = "ios")]
                display_fps: {
                    #[allow(deprecated)]
                    let fps = UIScreen::mainScreen(mtm).maximumFramesPerSecond();
                    fps.max(30) as u32
                },
                applied: Cell::new(0),
            })
        }

        /// The highest rate of the display the window is on.
        fn display_fps(&self) -> u32 {
            #[cfg(target_os = "ios")]
            return self.display_fps;
            #[cfg(target_os = "macos")]
            self.view.window().and_then(|w| w.screen()).map_or(60, |s| s.maximumFramesPerSecond().max(30) as u32)
        }

        /// Runs (at the cap in force) or pauses the link.
        pub fn run(&self, on: bool) {
            if on {
                let display = self.display_fps();
                let fps = match crate::max_fps() {
                    0 => display,
                    cap if cap >= display => display,
                    // A whole divisor of the display's rate (C# SnapMaxFpsToDisplay).
                    cap => display / display.div_ceil(cap),
                };
                if fps != self.applied.get() {
                    self.applied.set(fps);
                    let fps = fps as f32;
                    self.link.setPreferredFrameRateRange(CAFrameRateRange { minimum: fps, maximum: fps, preferred: fps });
                }
            }
            if self.link.isPaused() == on {
                self.link.setPaused(!on);
            }
        }
    }

    impl Drop for Ticker {
        fn drop(&mut self) {
            // The run loop keeps the link, the link its target.
            self.link.invalidate();
        }
    }
}

/// Frame timing, printed every 300 frames (logcat on Android): CPU time of our frame work, the
/// frame-to-frame interval, the animation step (how far the frame clock moved since the frame
/// before: what animations see; its spread is movement jitter), and what taking the pictures that
/// arrived cost the frame thread.
#[derive(Default)]
struct Stats {
    cpu_ms: Vec<f32>,
    interval_ms: Vec<f32>,
    step_ms: Vec<f32>,
    image_ms: Vec<f32>,
    last_start: Option<Instant>,
    last_time_ms: f64,
}

impl Stats {
    fn frame(&mut self, start: Instant, time_ms: f64) {
        self.cpu_ms.push(start.elapsed().as_secs_f32() * 1000.0);
        if let Some(last) = self.last_start.replace(start) {
            let interval = (start - last).as_secs_f32() * 1000.0;
            self.interval_ms.push(interval);
            // Steps of an animation, not the first frame after a pause.
            if interval < 100.0 {
                self.step_ms.push((time_ms - self.last_time_ms) as f32);
            }
        }
        self.last_time_ms = time_ms;
        if self.cpu_ms.len() == 300 {
            let (cpu, interval) = (summary(&mut self.cpu_ms), summary(&mut self.interval_ms));
            let mut line = format!("frames 300 | cpu ms {cpu} | interval ms {interval}");
            if self.step_ms.len() > 10 {
                let steps = &mut self.step_ms;
                steps.sort_by(f32::total_cmp);
                let at = |p: f32| steps[((steps.len() - 1) as f32 * p) as usize];
                line += &format!(" | step ms p5 {:.2} p50 {:.2} p95 {:.2} (spread {:.2})", at(0.05), at(0.5), at(0.95), at(0.95) - at(0.05));
            }
            if !self.image_ms.is_empty() {
                line += &format!(" | {} images, ms each {}", self.image_ms.len(), summary(&mut self.image_ms));
            }
            #[cfg(target_os = "android")]
            android::log(&line);
            #[cfg(not(target_os = "android"))]
            println!("{line}");
            self.cpu_ms.clear();
            self.interval_ms.clear();
            self.step_ms.clear();
            self.image_ms.clear();
        }
    }
}

fn summary(values: &mut [f32]) -> String {
    values.sort_by(f32::total_cmp);
    let at = |p: f32| values[((values.len() - 1) as f32 * p) as usize];
    format!("p50 {:.2} p95 {:.2} p99 {:.2} max {:.2}", at(0.5), at(0.95), at(0.99), at(1.0))
}
