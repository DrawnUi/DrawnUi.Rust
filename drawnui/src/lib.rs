//! DrawnUI for Rust: a Skia-drawn UI engine. A tree of controls is measured, arranged and painted
//! each frame; hosts supply the window or canvas, the GPU surface, input and assets.

pub use skia_safe as skia;

/// The version of this crate (an app's "about" line).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

mod animated_frames;
mod animators;
mod assets;
mod bakes;
mod control;
pub mod effects;
pub mod controls;
mod fonts;
mod gestures;
mod gpu;
mod images;
mod keyboard;
mod layout;
mod lottie;
mod paint;
pub mod testing;
mod tree;
mod types;
mod ui;

pub use animated_frames::{FramePlayer, FramesBuild, FramesSet};
pub use animators::{AnimationId, Easing, ValueAnimator, easing};
pub use bakes::BakeRequest;
pub use control::{Control, GestureCx, Handled, Has, LayoutCx, PaintCx};
pub use fonts::Fonts;
pub use gestures::{ContextMenuSource, Gesture, GestureKind, LONG_PRESS_MS, MouseButton, VelocityAccumulator};
pub use gpu::Gpu;
pub use images::{Frames, ImageRequest, Images};
pub use keyboard::{Cursor, InputKey, KEY_CODES, KeyEvent, KeyKind, Keyboard, Modifiers, key_name};
pub use tree::{Base, Build, Container, ControlId, ControlProps, Cx, Detached, Handle, IntoChildren, Mut, Tree};
pub use ui::{ACCESSIBILITY_INTERVAL_MS, AccessibilityNode, AccessibilityTextLine, AccessibilityValue, Aria, ContextMenu, FrameStats, HistoryOp};
pub use types::{
    CacheType, CornerRadius, Dirty, GesturesMode, GpuBackend, GradientType, IntoProp, LayoutOptions, LockTouch, RenderingModeType, SkiaGradient, SkiaShadow,
    Thickness,
};
pub use ui::Ui;

/// Everything an app needs: `use drawnui::prelude::*;`
pub mod prelude {
    pub use crate::controls::backdrop::{BackdropBuild, BackdropSet, SkiaBackdrop};
    pub use crate::controls::shader_carousel::{CarouselTransition, ShaderCarouselBuild, ShaderCarouselSet, SkiaShaderCarousel};
    pub use crate::controls::button::{ButtonBuild, ButtonLook, ButtonSet, SkiaButton, SkiaTouchAnimation};
    pub use crate::paint::CompositeRecord;
    pub use crate::effects::{
        CachedTexture, MultiRippleWithTouchEffect, SkiaEffect, SkiaShaderEffect, UseBackground, Uniforms,
        register_shader_source,
    };
    #[cfg(feature = "svg")]
    pub use crate::controls::checkbox::{CheckboxBuild, CheckboxSet, SkiaCheckbox};
    pub use crate::controls::control_style::PrebuiltControlStyle;
    pub use crate::controls::progress::{ProgressBuild, ProgressLook, ProgressSet, SkiaProgress};
    pub use crate::controls::radio_button::{RadioBuild, RadioSet, SkiaRadioButton};
    pub use crate::controls::slider::{RangeZone, SkiaSlider, SliderBuild, SliderLook, SliderSet};
    pub use crate::controls::switch::SkiaSwitch;
    pub use crate::controls::toggle::{SkiaToggle, ToggleBuild, ToggleSet};
    pub use crate::controls::gif::{GifBuild, GifSet, SkiaGif};
    pub use crate::controls::game::DrawnGame;
    pub use crate::controls::lottie::{LottieBuild, LottieSet, SkiaLottie};
    pub use crate::controls::rescaling_layout::{RescalingBuild, RescalingLayout, RescalingSet};
    pub use crate::controls::image::{
        DrawImageAlignment, FilterQuality, ImageBuild, ImageSet, SkiaImage, SkiaImageEffect, SkiaImageTiles, TilesBuild,
        TilesSet, TransformAspect,
    };
    pub use crate::controls::label::{
        FontAttributes, LabelBuild, LabelSet, LineBreakMode, SkiaLabel, TextAlignment, TextTransform,
    };
    pub use crate::controls::label_fps::SkiaLabelFps;
    pub use crate::controls::rich_label::{RichLabelBuild, RichLabelSet, SkiaRichLabel};
    pub use crate::controls::editor::{EditorBuild, EditorLook, EditorSet, KeyboardType, ReturnType, SkiaEditor};
    pub use crate::controls::text_span::{IntoSpans, TextSpan};
    pub use crate::controls::layout::{
        DecoratedGridBuild, DecoratedGridSet, GridLength, LayoutBuild, LayoutSet, LayoutType, MeasureBudget,
        MeasuringStrategy, RecyclingTemplate, SkiaDecoratedGrid, SkiaGrid, SkiaLayer, SkiaLayout, SkiaRow, SkiaStack,
        SkiaWrap,
    };
    pub use crate::controls::scroll::{
        RelativePositionType, ScrollBarVisibility, ScrollBuild, ScrollOrientation, ScrollSet, SkiaScroll, SnapToChildrenType,
    };
    pub use crate::controls::scroll_bar::{ScrollBarBuild, ScrollBarDock, ScrollBarSet, SkiaScrollBar};
    pub use crate::controls::carousel::{CarouselBuild, CarouselSet, SkiaCarousel};
    pub use crate::controls::drawer::{DrawerBuild, DrawerDirection, DrawerSet, SkiaDrawer};
    pub use crate::controls::snapping_layout::{SnappingBuild, SnappingSet};
    pub use crate::controls::shape::{BevelType, ShapeBuild, ShapeSet, ShapeType, SkiaBevel, SkiaShape};
    pub use crate::controls::shell::{
        ModalOptions, NavigationSource, PopupOptions, ShellArguments, ShellBuild, ShellNavigatedArgs, ShellNavigatingArgs,
        ShellSet, SkiaShell, build_route, split_route,
    };
    pub use crate::controls::sprite::{SkiaSprite, SkiaSpriteSet, SpriteBuild, SpriteSet, SpriteSetBuild, SpriteSetSet};
    #[cfg(feature = "svg")]
    pub use crate::controls::svg::{SkiaSvg, SvgBuild, SvgSet};
    pub use crate::skia::{BlendMode, Color, PaintCap, Point, Rect, Size, TileMode};
    pub use crate::{
        AccessibilityNode, AnimationId, Aria, Build, CacheType, FramesBuild, FramesSet, Container, ContextMenu, ContextMenuSource, Control,
        ControlId, CornerRadius, Cursor, Cx, Dirty, Easing, Gesture, GestureCx, GestureKind, GradientType, Handle, Handled,
        Has, InputKey, IntoChildren, KeyEvent, KeyKind, Keyboard, LayoutCx, LayoutOptions, LockTouch, Modifiers,
        MouseButton, Mut, PaintCx, GesturesMode, GpuBackend, RenderingModeType, SkiaGradient, SkiaShadow, Thickness, Ui, ValueAnimator, easing, props,
    };
}

#[cfg(not(target_os = "emscripten"))]
mod host_access;
#[cfg(not(target_os = "emscripten"))]
mod host_desktop;
#[cfg(target_os = "emscripten")]
mod host_web;

/// Everything an app may touch while drawing one frame.
pub struct Frame<'a> {
    pub surface: &'a mut skia::Surface,
    pub gpu: &'a mut Gpu,
    /// Size in pixels.
    pub width: f32,
    pub height: f32,
    /// Pixels per point.
    pub scale: f32,
    /// Vsync-aligned time in milliseconds.
    pub time_ms: f64,
    pub host: &'a mut Host,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PointerKind {
    Down,
    Move,
    Up,
    /// The press was taken away without a release: the browser took the touch for the page, the
    /// window lost the pointer. It ends the gesture; it is not a tap and starts no fling.
    Cancel,
    /// The mouse moved with no button down (a touch never hovers). Only the last one before a
    /// frame is looked at.
    Hover,
    /// The mouse left the window or the canvas: nothing is hovered anymore.
    Leave,
}

/// Requests from the app to the host.
#[derive(Default)]
pub struct Host {
    requests: Vec<(u32, String)>,
    images: Vec<ImageRequest>,
    /// Frame time the app wants a frame at when it asked for none (a timer, a sleeping animator).
    wake_ms: Option<f64>,
    /// The mouse cursor changed.
    cursor: Option<Cursor>,
    /// The text input changed: `Some(rect)` = a control that takes typed text is focused, the
    /// host opens its keyboard / IME there (rect in points); `None` = nobody takes text.
    text_input: Option<Option<skia::Rect>>,
    /// The accessibility snapshot changed: the host renders it (the web overlay).
    accessibility: Option<Vec<AccessibilityNode>>,
    /// Set by a host that renders the accessibility snapshot: the app builds it only then.
    pub(crate) accessibility_on: bool,
    /// Set by a host with no keyboard navigation of its own (the desktop; the browser's overlay
    /// has the page's): Tab, arrows in a group, Enter / Space, Escape and the focus ring.
    pub(crate) keyboard_navigation: bool,
    /// The node of the control that takes typed text (`text_input`), the snapshot `id`.
    pub(crate) text_input_node: Option<u32>,
    /// The engine moved the keyboard to this node (an arrow in a group): an overlay host moves its
    /// focus there.
    pub(crate) keyboard_moved: Option<u32>,
    /// Text for the system clipboard.
    clipboard: Option<String>,
    /// The app wants the clipboard's text as a `KeyKind::Char`.
    paste: bool,
    /// Links to open in the system browser.
    urls: Vec<String>,
    /// Changes to the browser's history, in order.
    pub(crate) history: Vec<HistoryOp>,
    /// Set by a host with a browser history (the web).
    pub(crate) history_on: bool,
    /// Set by a host that draws ImageDoubleBuffered caches on worker threads (the desktop).
    pub(crate) bake_workers: bool,
    /// Pictures of ImageDoubleBuffered caches for the workers; the bitmaps come back through
    /// [`App::baked`].
    pub(crate) bakes: Vec<BakeRequest>,
}

/// A picture the host decoded, ready to draw.
pub struct Decoded {
    /// A raster image: its pixels are in memory, nothing is left to decode and it is no texture.
    /// For an animation, its first frame.
    pub image: skia::Image,
    /// Pixel size of the file; `image` is smaller when the request asked for less.
    pub source_size: skia::ISize,
    /// Every frame, when the request asked for them and the file has more than one.
    pub frames: Option<std::sync::Arc<Frames>>,
}

impl Host {
    /// Asks for the bytes of `url`; they arrive later through [`App::asset`] with the same `id`.
    /// Desktop reads the path relative to the working directory, web fetches it.
    pub fn fetch(&mut self, id: u32, url: &str) {
        self.requests.push((id, url.to_owned()));
    }

    /// Asks for a picture, decoded off the frame thread (see [`ImageRequest`]). It arrives
    /// through [`App::image`].
    pub fn fetch_image(&mut self, request: ImageRequest) {
        self.images.push(request);
    }

    /// Asks for a mouse cursor; the host keeps it until the next request.
    pub fn set_cursor(&mut self, cursor: Cursor) {
        self.cursor = Some(cursor);
    }

    /// Tells the host whether a control takes typed text, and where (points): the browser focuses
    /// its hidden input there (soft keyboard, IME, paste), the desktop allows the IME. `None` closes it.
    pub fn set_text_input(&mut self, area: Option<skia::Rect>) {
        self.text_input = Some(area);
    }

    /// Hands the host a new accessibility snapshot to render (the web overlay); an empty one
    /// removes every node.
    pub fn set_accessibility(&mut self, nodes: Vec<AccessibilityNode>) {
        self.accessibility = Some(nodes);
    }

    /// Puts text on the system clipboard (browser: `navigator.clipboard.writeText`).
    pub fn set_clipboard(&mut self, text: String) {
        self.clipboard = Some(text);
    }

    /// Opens a link in the system browser (web: a new tab). Only http, https and mailto links:
    /// anything else could start a program on the desktop.
    pub fn open_url(&mut self, url: &str) {
        let scheme = url.split_once(':').map(|(s, _)| s.to_ascii_lowercase());
        match scheme.as_deref() {
            Some("http" | "https" | "mailto") => self.urls.push(url.to_owned()),
            _ => eprintln!("drawnui: not opening {url}: only http, https and mailto links"),
        }
    }

    /// Asks for the clipboard's text, delivered through [`App::key`] as `KeyKind::Char`.
    pub fn request_paste(&mut self) {
        self.paste = true;
    }
}

pub trait App {
    fn init(&mut self, _host: &mut Host) {}
    /// GPU or CPU; the host asks once, before the first frame.
    fn rendering_mode(&self) -> RenderingModeType {
        RenderingModeType::Accelerated
    }
    /// The GPU API; the host asks once, before its window has a surface.
    fn gpu_backend(&self) -> GpuBackend {
        GpuBackend::Auto
    }
    /// The canvas background (`Ui::background`): a host that owns a native window paints the
    /// surface behind the canvas with it, so nothing of another color shows before the first frame.
    fn background(&self) -> skia::Color {
        skia::Color::TRANSPARENT
    }
    /// What the canvas does with gestures the page could take (browser); asked once at start.
    fn gestures(&self) -> GesturesMode {
        GesturesMode::Enabled
    }
    /// Draws one frame. Returns true when another frame is needed.
    fn frame(&mut self, frame: &mut Frame) -> bool;
    /// Coordinates in pixels. `time_ms` is when the event arrived, on the clock of `Frame::time_ms`.
    fn pointer(&mut self, _kind: PointerKind, _x: f32, _y: f32, _time_ms: f64) {}
    /// Pointer input of any mouse button; `pointer` is the left one. Default: only the left
    /// button is delivered.
    fn pointer_button(&mut self, kind: PointerKind, button: MouseButton, x: f32, y: f32, time_ms: f64) {
        if button == MouseButton::Left {
            self.pointer(kind, x, y, time_ms);
        }
    }
    /// Pointer input of a finger. Default: delivered as the left button.
    fn pointer_touch(&mut self, kind: PointerKind, x: f32, y: f32, time_ms: f64) {
        self.pointer_button(kind, MouseButton::Left, x, y, time_ms);
    }
    /// A context menu was asked for over the pixel (x, y) (a right click, a long press on touch,
    /// the Menu key). Answered at once: true when a control took it, the browser then keeps its own
    /// menu away.
    fn context_menu(&mut self, _x: f32, _y: f32, _source: ContextMenuSource, _time_ms: f64) -> bool {
        false
    }
    /// A key went down or up (`key` is its DOM `code` name), or text was typed (`KeyKind::Char`,
    /// `text`). Answered at once: true when the app used it (the browser then keeps its default
    /// action away, the page does not scroll on Space).
    fn key(&mut self, _kind: KeyKind, _key: InputKey, _text: &str, _modifiers: Modifiers, _repeat: bool) -> bool {
        false
    }
    /// The window lost the keyboard: no key counts as held anymore.
    fn blur(&mut self) {}
    /// The page's focus went to an element outside the canvas (a field of the page): no drawn
    /// control keeps the keyboard (the browser host).
    fn focus_out(&mut self) {}
    /// The system's Back (Android's back button or gesture) that no key handler used. True when
    /// the app went back (a shell closed a popup, a modal or a page); false lets the system have it.
    fn back(&mut self) -> bool {
        false
    }
    /// The accessibility overlay activated a control (a click on its node, Enter or Space on it):
    /// the control gets a Tapped at its center. `id` is `AccessibilityNode::id`.
    fn accessibility_activate(&mut self, _id: u32) {}
    /// The node the keyboard (or a screen reader) is on, `None` = none.
    fn accessibility_focused_id(&self) -> Option<u32> {
        None
    }
    /// The accessibility overlay moved its keyboard focus onto (`focused`) or off a control.
    fn accessibility_focus(&mut self, _id: u32, _focused: bool) {}
    /// A screen reader's Increment (`up`) or Decrement on a range control (a slider).
    fn accessibility_adjust(&mut self, _id: u32, _up: bool) {}
    /// A screen reader set a range control's value.
    fn accessibility_set_value(&mut self, _id: u32, _value: f64) {}
    /// A screen reader paged a scroll's node: `x` / `y` 1 forward (right, down), -1 back.
    fn accessibility_scroll(&mut self, _id: u32, _x: f32, _y: f32) {}
    /// A screen reader asked to scroll a node into view.
    fn accessibility_scroll_into_view(&mut self, _id: u32) {}
    /// The page or window became hidden (`false`: a background tab, a minimized or covered
    /// window) or visible again (browser `visibilitychange`, winit `Occluded`).
    fn visibility(&mut self, _visible: bool) {}
    /// The safe area of the screen in points (browser `env(safe-area-inset-*)`), at start and
    /// when it changes. The desktop never calls it.
    fn safe_insets(&mut self, _insets: Thickness) {}
    /// The browser's URL hash: at start (`popped` None), and after the browser moved in its
    /// history (`popped` = the depth its entry was pushed with, 0 for one the app did not push).
    fn location(&mut self, _hash: &str, _popped: Option<u32>) {}
    /// Mouse wheel or touchpad scroll over the pixel (x, y). `delta` is the dominant axis in
    /// notches (a touchpad sends fractions), `horizontal` says which axis that is; positive =
    /// toward the start (up, or left). The host needs the answer at once: true when the app used
    /// the wheel (the browser then keeps the page from scrolling with it).
    fn wheel(&mut self, _x: f32, _y: f32, _delta: f32, _horizontal: bool, _time_ms: f64) -> bool {
        false
    }
    /// The bytes of a fetched asset; empty when it could not be loaded.
    fn asset(&mut self, _id: u32, _bytes: Vec<u8>) {}
    /// The picture of a `Host::fetch_image` request; `None` when it could not be loaded or decoded.
    fn image(&mut self, _id: u32, _decoded: Option<Decoded>) {}
    /// The bitmap a worker made of a [`BakeRequest`]; `None` when none could be made.
    fn baked(&mut self, _id: u32, _image: Option<skia::Image>) {}
}

static MAX_FPS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The frame-rate cap (DrawnUI `Super.MaxFps`). 0, the default: the display's own rate, 120 Hz
/// on a ProMotion iPhone. 60 keeps such a phone at 60. A cap the display cannot present is
/// snapped to a whole divisor of its rate (120 Hz: 120, 60, 40, 30...). Can change at any time.
/// The iOS host paces its display link with it; the other hosts do not read it yet.
pub fn set_max_fps(fps: u32) {
    MAX_FPS.store(fps, std::sync::atomic::Ordering::Relaxed);
}

/// The frame-rate cap set with [`set_max_fps`]; 0 = none.
pub fn max_fps() -> u32 {
    MAX_FPS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Starts the app. Desktop: runs the window loop until close. Web: registers the app and returns;
/// `drawnui_host.js` drives it from there.
/// The Android activity (winit's NativeActivity), what an app's `android_main` receives.
#[cfg(target_os = "android")]
pub use winit::platform::android::activity::AndroidApp;

/// Android: hands the activity to the host. An app's `android_main` calls it before `run`, for
/// every activity: Android may destroy one and start another in the same process.
#[cfg(target_os = "android")]
pub fn set_android_app(app: AndroidApp) {
    host_desktop::set_android_app(app);
}

pub fn run(title: &str, make: fn() -> Box<dyn App>) {
    run_sized(title, 1000.0, 700.0, make);
}

/// `run` with the desktop window's first size in points; the web canvas keeps the page's size.
pub fn run_sized(title: &str, width: f64, height: f64, make: fn() -> Box<dyn App>) {
    #[cfg(not(target_os = "emscripten"))]
    host_desktop::run(title, (width, height), make);
    #[cfg(target_os = "emscripten")]
    {
        let _ = (width, height);
        host_web::run(title, make);
    }
}
