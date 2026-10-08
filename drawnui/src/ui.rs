//! `Ui` ties a control tree to the app state and runs the frame: input, observers, invalidation,
//! layout, paint. It is an `App`, so any host can drive it. Input: the gesture router (with the
//! gesture owner, hover, long press, context menu), the keyboard with its focused control, and the
//! accessibility snapshot.

use std::any::Any;
use std::collections::VecDeque;

use skia_safe::{Canvas, Color, Matrix, Paint, Point, RRect, Rect, Size};

use crate::control::{GestureCx, Handled, PaintCx};
use crate::fonts::Fonts;
use crate::gestures::{ContextMenuSource, Gesture, GestureKind, MouseButton, Recognizer};
use crate::gpu::Gpu;
use crate::keyboard::{Cursor, InputKey, KeyEvent, KeyKind, Keyboard, Modifiers};
use crate::tree::{Build, ControlId, Cx, Detached, Raw, Tree};
use crate::types::{Dirty, GesturesMode, GpuBackend, LockTouch, RenderingModeType};
use crate::controls::label::LabelSet;
use crate::controls::label_fps::SkiaLabelFps;
use crate::{App, Control, Decoded, Frame, Host, PointerKind, animators, images, layout, paint};

/// How often the accessibility snapshot is rebuilt at most (React MinUpdateIntervalMs).
pub const ACCESSIBILITY_INTERVAL_MS: f64 = 1000.0;

// ---------------------------------------------------------------- handler types

pub(crate) type LongPressed = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>)>;
pub(crate) type ContextMenuHandler = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>, &ContextMenu) -> bool>;
pub(crate) type KeyHandler = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>, &KeyEvent<'_>) -> bool>;
pub(crate) type FocusHandler = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>, bool)>;
pub(crate) type GestureHandler = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>, &Gesture) -> bool>;

/// The per-control input handlers besides `tapped`, boxed together on the node.
#[derive(Default)]
pub(crate) struct InputHandlers {
    pub long_pressing: Option<LongPressed>,
    pub context_menu: Option<ContextMenuHandler>,
    pub key_down: Option<KeyHandler>,
    pub key_up: Option<KeyHandler>,
    pub key_char: Option<KeyHandler>,
    pub focus_changed: Option<FocusHandler>,
    /// `Build::on_hovered`: the control's `is_hovered` changed.
    pub hovered: Option<FocusHandler>,
    /// Sees every gesture first (`Build::consume_gestures`).
    pub consume_gestures: Option<GestureHandler>,
    /// Gets every key while mounted (`Build::listen_keys`).
    pub listen_keys: bool,
    /// Gets the browser's history moves while mounted (`Build::listen_history`).
    pub listen_history: bool,
}

/// A context menu request (React ContextMenuEventArgs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContextMenu {
    /// The point on the canvas, in points.
    pub location: Point,
    /// The same point in pixels.
    pub pixels: Point,
    pub source: ContextMenuSource,
    /// The deepest control under the point with a `on_context_menu` handler; `None` for the
    /// app-level fallback.
    pub control: Option<ControlId>,
    /// The point inside `control`, in pixels from its drawing rect's origin.
    pub local: Point,
}

/// A change to the browser's history (React SkiaShell UseBrowserHistory); the desktop has none.
#[derive(Clone, Debug, PartialEq)]
pub enum HistoryOp {
    /// `history.pushState({ depth }, "", hash)`; `None` keeps the URL, an empty hash gives the
    /// URL without one (pathname + search).
    Push { depth: u32, hash: Option<String> },
    /// `history.replaceState({ depth }, "", hash)`.
    Replace { depth: u32, hash: Option<String> },
    /// `history.back()`: the browser answers with `Control::on_history`.
    Back,
}

/// Timing of the drawn frames (React Canvas.FrameTime / FPS), for an app's diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameStats {
    /// Milliseconds the last frame took on the CPU: input, animators, layout, paint; no GPU flush.
    pub frame_time_ms: f32,
    /// Frames drawn during the last second of the frame clock (the last value while idle).
    pub fps: u32,
}

/// ARIA role and live-region names for `accessibility_role` / `accessibility_live` (DrawnUI Aria).
pub struct Aria;
impl Aria {
    pub const BUTTON: &'static str = "button";
    pub const LINK: &'static str = "link";
    pub const CHECKBOX: &'static str = "checkbox";
    pub const RADIO: &'static str = "radio";
    pub const SWITCH: &'static str = "switch";
    pub const SLIDER: &'static str = "slider";
    pub const SPIN_BUTTON: &'static str = "spinbutton";
    pub const TEXT_BOX: &'static str = "textbox";
    pub const SEARCH_BOX: &'static str = "searchbox";
    pub const COMBO_BOX: &'static str = "combobox";
    pub const LIST_BOX: &'static str = "listbox";
    pub const OPTION: &'static str = "option";
    pub const TAB: &'static str = "tab";
    pub const TAB_PANEL: &'static str = "tabpanel";
    pub const TAB_LIST: &'static str = "tablist";
    pub const MENU: &'static str = "menu";
    pub const MENU_ITEM: &'static str = "menuitem";
    pub const MENU_ITEM_CHECKBOX: &'static str = "menuitemcheckbox";
    pub const MENU_ITEM_RADIO: &'static str = "menuitemradio";
    pub const SCROLL_BAR: &'static str = "scrollbar";
    pub const TEXT: &'static str = "text";
    pub const HEADING: &'static str = "heading";
    pub const IMG: &'static str = "img";
    pub const LIST: &'static str = "list";
    pub const LIST_ITEM: &'static str = "listitem";
    pub const SEPARATOR: &'static str = "separator";
    pub const PROGRESS_BAR: &'static str = "progressbar";
    pub const TOOLTIP: &'static str = "tooltip";
    pub const DIALOG: &'static str = "dialog";
    pub const ALERT_DIALOG: &'static str = "alertdialog";
    pub const STATUS: &'static str = "status";
    pub const ALERT: &'static str = "alert";
    pub const GROUP: &'static str = "group";
    pub const REGION: &'static str = "region";
    pub const NAVIGATION: &'static str = "navigation";
    pub const MAIN: &'static str = "main";
    pub const GRID: &'static str = "grid";
    pub const TOOLBAR: &'static str = "toolbar";
    pub const RADIO_GROUP: &'static str = "radiogroup";
    pub const MENU_BAR: &'static str = "menubar";
    /// Keeps the control out of the accessibility tree.
    pub const PRESENTATION: &'static str = "presentation";
    /// Not ARIA: a SkiaScroll's node in a screen reader's tree, which pages it (VoiceOver's
    /// three-finger swipe, TalkBack's scroll forward / back). The browser overlay leaves it out.
    pub const SCROLL_VIEW: &'static str = "scrollview";
    pub const LIVE_POLITE: &'static str = "polite";
    pub const LIVE_ASSERTIVE: &'static str = "assertive";
}

/// One laid-out text line of a control with `accessibility_text_selectable` (React
/// AccessibilityTextLine). Points, from the node's top-left.
#[derive(Clone, Debug, PartialEq)]
pub struct AccessibilityTextLine {
    pub text: String,
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
    /// The font alias and its fallbacks, comma-separated.
    pub font_family: String,
    pub font_weight: i32,
    /// Points.
    pub font_size: f32,
}

/// The value of a range control (a slider, a progress bar): what a screen reader reads and
/// adjusts. UI Automation RangeValue, NSAccessibility value, aria-valuenow / min / max / valuetext.
#[derive(Clone, Debug, PartialEq)]
pub struct AccessibilityValue {
    pub now: f64,
    pub min: f64,
    pub max: f64,
    /// One Increment / Decrement (an arrow key).
    pub step: f64,
    /// Spoken instead of the number when not empty: "65%", "20 – 80".
    pub text: String,
}

/// One control of the accessibility snapshot (React AccessibilityNode): what the host's overlay
/// renders as an ARIA element over the canvas.
#[derive(Clone, Debug, PartialEq)]
pub struct AccessibilityNode {
    /// Stable identity for the overlay; `App::accessibility_activate` takes it.
    pub id: u32,
    pub control: ControlId,
    pub role: &'static str,
    pub label: String,
    pub hint: String,
    /// On the canvas, in points.
    pub rect: Rect,
    /// A tab stop that Enter, Space or a click activates.
    pub can_interact: bool,
    pub is_pressed: Option<bool>,
    /// A range control's value.
    pub value: Option<AccessibilityValue>,
    /// A scroll's node: the axes its content can move along (horizontal, vertical).
    pub scrolls: (bool, bool),
    pub live: &'static str,
    /// Its text lines when the control has `accessibility_text_selectable`; empty otherwise.
    pub text_lines: Vec<AccessibilityTextLine>,
    /// The nearest node above it (`id`), `None` under the root: a screen reader's tree nests
    /// a card's items in it (the web overlay is flat).
    pub parent: Option<u32>,
    /// The arrow-key group it is in: the group's `id` and the control index of the group's item
    /// holding it. An overlay keeps a group one Tab stop with it (roving tabindex).
    pub group: Option<(u32, u32)>,
}

/// A role a user operates (a button, a field, a toggle...): one that takes no input is read as
/// unavailable (aria-disabled, UI Automation IsEnabled false).
pub(crate) fn is_control_role(role: &str) -> bool {
    matches!(
        role,
        Aria::BUTTON | Aria::LINK | Aria::CHECKBOX | Aria::RADIO | Aria::SWITCH | Aria::SLIDER | Aria::SPIN_BUTTON | Aria::TEXT_BOX | Aria::SEARCH_BOX | Aria::COMBO_BOX | Aria::OPTION | Aria::TAB | Aria::MENU_ITEM | Aria::MENU_ITEM_CHECKBOX | Aria::MENU_ITEM_RADIO
    )
}

/// A text or a heading inside the node says its name: the node is not named again (a card and
/// its title). A selectable text stays itself.
pub(crate) fn said_by_child(nodes: &[AccessibilityNode], node: &AccessibilityNode) -> bool {
    nodes.iter().any(|c| c.parent == Some(node.id) && (c.role == Aria::TEXT || c.role == Aria::HEADING) && c.text_lines.is_empty() && c.label == node.label)
}

type UiKeyHandler<S> = Box<dyn FnMut(&mut S, &KeyEvent<'_>, &mut Cx<'_>) -> bool>;

pub struct Ui<S: 'static> {
    pub tree: Tree,
    pub state: S,
    pub fonts: Fonts,
    /// Painted under the root control.
    pub background: Color,
    /// GPU or CPU (DrawnUI `Canvas.RenderingMode`), read by the host before the first frame.
    rendering_mode: RenderingModeType,
    /// The GPU API, read by the host before its window has a surface.
    gpu_backend: GpuBackend,
    /// DrawnUI `Canvas.Gestures`, read by the browser host at start.
    gestures: GesturesMode,
    /// (kind, button, location, arrival time in ms).
    input: VecDeque<(PointerKind, MouseButton, bool, Point, f64)>,
    recognizer: Recognizer,
    /// The control that consumed the current gesture keeps receiving it until it lets go.
    owner: Option<ControlId>,
    state_dirty: bool,
    /// Fonts the host loads: (alias, url, weight, the first layout waits for it).
    /// Alias, url, weight, whether the first layout waits for it, whether it came from the page.
    font_urls: Vec<(String, String, i32, bool, bool)>,
    /// Registered fonts the host has not delivered yet. Nothing is laid out before they arrive.
    fonts_pending: usize,
    scale: f32,
    fps: Fps,
    /// DrawnUI `Super.InsetsChanged`: the host reported another safe area.
    insets_handler: Option<Box<dyn FnMut(&mut S, crate::types::Thickness, &mut Cx<'_>)>>,
    /// The modifiers and held keys (React KeyboardManager).
    pub keyboard: Keyboard,
    /// App-level key handlers (React KeyboardManager subscribers), run after the focused control.
    key_down: Vec<UiKeyHandler<S>>,
    key_up: Vec<UiKeyHandler<S>>,
    key_char: Vec<UiKeyHandler<S>>,
    /// The app-level context menu fallback (React Canvas.ContextMenu).
    context_menu: Option<Box<dyn FnMut(&mut S, &ContextMenu, &mut Cx<'_>) -> bool>>,
    /// Runs when the page or window is hidden or shown (`on_visibility_changed`).
    visibility: Option<Box<dyn FnMut(&mut S, bool, &mut Cx<'_>)>>,
    /// Runs when the canvas size changes (`on_canvas_resized`).
    resized: Option<Box<dyn FnMut(&mut S, Size, &mut Cx<'_>)>>,
    /// The host reported other safe-area insets: the resize handler runs again.
    insets_changed: bool,
    /// Frame clock times of the frames of the last second, oldest first (`FrameStats::fps`).
    frame_times: VecDeque<f64>,
    /// What `prepare` left for the paint of the same frame: its time, the content rect, when the
    /// frame's work started, whether a tree was laid out.
    prepared: Option<(f64, Rect, std::time::Instant, bool)>,
    /// The controls under the mouse, root first (DrawnUI IsPointerOver), and the list being
    /// collected by the current hover route; swapped, never reallocated.
    over: Vec<ControlId>,
    over_next: Vec<ControlId>,
    /// The controls under the mouse that take hover (DrawnUI HoveredControls), root first, and
    /// the list being built; swapped, never reallocated.
    hovered: Vec<ControlId>,
    hovered_next: Vec<ControlId>,
    /// Where the mouse was last seen over the canvas (`None` = it left).
    last_pointer: Option<Point>,
    /// A scroll, carousel or drawer moved its content in the last frame: hover waits.
    hover_paused: bool,
    cursor: Cursor,
    cursor_changed: bool,
    /// The last text input area told to the host (`None` = closed), and whether it changed.
    text_input: Option<Rect>,
    text_input_changed: bool,
    /// When the accessibility snapshot (`Tree::accessibility`) was built, and whether the host
    /// has not seen it yet.
    accessibility_built_ms: f64,
    accessibility_changed: bool,
    /// The host renders the snapshot (the web overlay); off, nothing is built.
    accessibility_on: bool,
    /// A frame was drawn inside the interval: the snapshot may be behind it.
    accessibility_stale: bool,
    /// The walk's stack and the next snapshot, kept for their allocations.
    accessibility_stack: Vec<(ControlId, Matrix, Option<u32>)>,
    /// The host has no keyboard navigation of its own (`Host::keyboard_navigation`).
    keyboard_navigation: bool,
    /// The focus ring shows: the keyboard moved the focus; a pointer press hides it.
    focus_ring: bool,
    /// Per arrow-key group, the item the keyboard was on (C# NoteFocus): Tab comes back to it.
    group_items: Vec<(ControlId, ControlId)>,
    /// The engine moved the keyboard to this node (an arrow in a group): the web overlay moves
    /// the page's focus there.
    keyboard_moved: Option<u32>,
    accessibility_next: Vec<AccessibilityNode>,
    /// Snapshots that differed from the one before (React AccessibilityManager.Changed).
    accessibility_revision: u32,
    /// Roles by control type (`default_accessibility_role`).
    accessibility_roles: Vec<(std::any::TypeId, &'static str)>,
    /// Children sorted by z order while the router walks them (`Routing::by_z`).
    by_z: Vec<ControlId>,
}

/// The FPS overlay: frames per second over the last half second of frames the engine itself
/// asked for, one after the other (an animation, a scroll). Frames are drawn on demand: a frame
/// that input or a timer woke up does not count, and an idle app keeps the last number measured.
#[derive(Default)]
struct Fps {
    on: bool,
    /// The last frame ended asking for the next one.
    chained: bool,
    since_ms: f64,
    frames: u32,
    value: f32,
    /// The value the SkiaLabelFps labels show.
    shown: f32,
}

impl Fps {
    /// Counts the frame; the value changes every half second of chained frames.
    fn measure(&mut self, time_ms: f64) {
        if !self.chained {
            (self.since_ms, self.frames) = (time_ms, 0);
        }
        self.frames += 1;
        let span = time_ms - self.since_ms;
        if span >= 500.0 {
            self.value = ((self.frames - 1) as f64 * 1000.0 / span) as f32;
            (self.since_ms, self.frames) = (time_ms, 1);
        }
    }

    fn draw(&mut self, canvas: &Canvas, fonts: &Fonts, height: f32, scale: f32) {
        let Some(font) = fonts.font("", 16.0 * scale) else { return };
        let text = if self.value > 0.0 { format!("FPS {:.0}", self.value) } else { "FPS -".to_owned() };
        let (text_width, _) = font.measure_str(&text, None);
        // Bottom left, 8 points from the edges: the top corners belong to the shell's buttons.
        let pill = Rect::from_xywh(8.0 * scale, height - 34.0 * scale, text_width + 20.0 * scale, 26.0 * scale);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(Color::from_argb(220, 0, 0, 0));
        canvas.draw_rrect(RRect::new_rect_xy(pill, 6.0 * scale, 6.0 * scale), &paint);
        paint.set_color(Color::from_argb(255, 80, 255, 120));
        canvas.draw_str(text, (pill.left + 10.0 * scale, pill.top + 19.0 * scale), &font, &paint);
    }
}

impl<S: 'static> Ui<S> {
    /// Builds the tree once. `build` gets the state so it can store handles with `assign`.
    pub fn new<B: Into<Detached>>(mut state: S, build: impl FnOnce(&mut S) -> B) -> Self {
        let mut tree = Tree::default();
        tree.mount(None, build(&mut state));
        Self {
            tree,
            state,
            fonts: Fonts::default(),
            background: Color::TRANSPARENT,
            rendering_mode: RenderingModeType::Accelerated,
            gpu_backend: GpuBackend::Auto,
            gestures: GesturesMode::Enabled,
            input: VecDeque::new(),
            recognizer: Recognizer::default(),
            owner: None,
            state_dirty: true,
            font_urls: Vec::new(),
            fonts_pending: 0,
            scale: 1.0,
            fps: Fps::default(),
            insets_handler: None,
            keyboard: Keyboard::default(),
            key_down: Vec::new(),
            key_up: Vec::new(),
            key_char: Vec::new(),
            context_menu: None,
            resized: None,
            visibility: None,
            insets_changed: false,
            frame_times: VecDeque::with_capacity(256),
            prepared: None,
            over: Vec::with_capacity(32),
            over_next: Vec::with_capacity(32),
            hovered: Vec::with_capacity(8),
            hovered_next: Vec::with_capacity(8),
            last_pointer: None,
            hover_paused: false,
            cursor: Cursor::Default,
            cursor_changed: false,
            text_input: None,
            text_input_changed: false,
            accessibility_built_ms: f64::NEG_INFINITY,
            accessibility_changed: false,
            accessibility_on: false,
            accessibility_stale: false,
            accessibility_stack: Vec::new(),
            keyboard_navigation: false,
            focus_ring: false,
            group_items: Vec::new(),
            keyboard_moved: None,
            accessibility_next: Vec::new(),
            accessibility_revision: 0,
            accessibility_roles: Vec::new(),
            by_z: Vec::new(),
        }
    }

    /// Registers a font the host loads (a file on the desktop, a fetch on the web), at the
    /// regular weight. The first font registered is the default one; nothing is laid out before
    /// it arrived.
    pub fn font(self, alias: &str, url: &str) -> Self {
        self.font_weight(alias, url, 400)
    }

    /// Registers a face of a family at a weight (100..900), loaded by the host like `font`:
    /// `font_weight` and bold on a label pick the nearest registered weight (DrawnUI
    /// `fonts.AddFont(source, alias, weight)`).
    pub fn font_weight(mut self, alias: &str, url: &str, weight: i32) -> Self {
        // Registration order decides the default font, not the order the files arrive in.
        self.fonts.register(alias);
        self.font_urls.push((alias.to_owned(), url.to_owned(), weight, true, false));
        self.fonts_pending += 1;
        self
    }

    /// Registers a font the first layout does not wait for (a large emoji or symbols font named
    /// in `font_family_fallback`): texts are measured again when it arrives. Register the fonts
    /// the first layout needs before it, or it could become the default font.
    pub fn font_fallback(mut self, alias: &str, url: &str) -> Self {
        self.font_urls.push((alias.to_owned(), url.to_owned(), 400, false, false));
        self
    }

    /// Registers a font from bytes already in memory.
    pub fn font_bytes(mut self, alias: &str, bytes: &[u8]) -> Self {
        self.fonts.add(alias, bytes);
        self
    }

    /// Loads image sources from the start, ahead of the page that shows them, so it opens with its
    /// pictures (DrawnUI `PreloadImages` at app start). They go to the host after everything a
    /// control waits for, and stay cached like any loaded picture.
    pub fn preload_images<I: AsRef<str>>(mut self, sources: impl IntoIterator<Item = I>) -> Self {
        self.tree.images.preload(sources);
        self
    }

    pub fn background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    /// DrawnUI `Canvas.RenderingMode`: Accelerated (the default, as DrawnUi.React) draws on the
    /// GPU; Default draws each frame on the CPU, caches as CPU bitmaps. Read once, before the
    /// first frame. A browser without WebGL2 draws on the CPU whatever is set.
    pub fn rendering_mode(mut self, mode: RenderingModeType) -> Self {
        self.rendering_mode = mode;
        self
    }

    /// The GPU API: Auto (the default) is Vulkan on Android with OpenGL ES where Vulkan cannot
    /// be made, the platform's own elsewhere. On Android the system property `debug.drawnui.gpu`
    /// (`gl` or `vulkan`, `adb shell setprop`) overrides it, for tests and benchmarks.
    pub fn gpu_backend(mut self, backend: GpuBackend) -> Self {
        self.gpu_backend = backend;
        self
    }

    /// DrawnUI `Canvas.Gestures`. Enabled (the default) shares with the page what the app does not
    /// use; Lock makes the canvas own every touch in the browser (no page scroll, bounce, Safari's
    /// pull-down or text selection starting on it): for full-screen apps and games. Read once, at
    /// start; the native hosts ignore it.
    pub fn gestures(mut self, mode: GesturesMode) -> Self {
        self.gestures = mode;
        self
    }

    /// DrawnUI `MobileIsFullscreen`, every platform with a safe area (Android, iOS, a phone's
    /// browser). False, the default as in DrawnUI: the root is laid out inside the safe area and
    /// the strips under the system bars show the background. True: the root covers the whole
    /// screen, under the status and navigation bars; a `SkiaShell` keeps its bars and pages out
    /// of them, other content uses `Cx::safe_insets`. Either way `Cx::safe_insets` is the safe
    /// area the platform reports (DrawnUI `Super.Screen` insets). No effect without a safe area
    /// (the desktop).
    pub fn mobile_fullscreen(mut self, on: bool) -> Self {
        self.tree.mobile_fullscreen = on;
        self
    }

    /// DrawnUI `Super.InsetsChanged`: the platform reported another safe area (a phone turned, the
    /// navigation bar changed). Runs before the frame's layout; `Cx::safe_insets` has the new one.
    pub fn on_safe_insets_changed(mut self, f: impl FnMut(&mut S, crate::types::Thickness, &mut Cx<'_>) + 'static) -> Self {
        self.insets_handler = Some(Box::new(f));
        self
    }

    /// Shows the frames per second in the bottom left corner (8 points from the edges), above everything.
    pub fn show_fps(mut self, on: bool) -> Self {
        self.fps.on = on;
        self
    }

    /// A window-level key handler (React KeyboardManager.Subscribe): every key down, after the
    /// focused control had it. True = used (the host keeps its default action away).
    pub fn on_key_down(mut self, f: impl FnMut(&mut S, &KeyEvent<'_>, &mut Cx<'_>) -> bool + 'static) -> Self {
        self.key_down.push(Box::new(f));
        self
    }

    /// A window-level key up handler.
    pub fn on_key_up(mut self, f: impl FnMut(&mut S, &KeyEvent<'_>, &mut Cx<'_>) -> bool + 'static) -> Self {
        self.key_up.push(Box::new(f));
        self
    }

    /// A window-level handler for typed text (React KeyChar): `event.text`.
    pub fn on_key_char(mut self, f: impl FnMut(&mut S, &KeyEvent<'_>, &mut Cx<'_>) -> bool + 'static) -> Self {
        self.key_char.push(Box::new(f));
        self
    }

    /// The context menu fallback (React Canvas.ContextMenu): runs when no control took the
    /// request. True = taken, the browser's own menu stays away.
    pub fn on_context_menu(mut self, f: impl FnMut(&mut S, &ContextMenu, &mut Cx<'_>) -> bool + 'static) -> Self {
        self.context_menu = Some(Box::new(f));
        self
    }

    /// Runs with the canvas size in points before the first layout and whenever it or the safe
    /// area (`Cx::safe_insets`) changes (a window resized, a phone turned); observers run after it (React `useCardWidth`: one column
    /// below 640 points). `Cx::canvas_size` gives the size in any handler.
    pub fn on_canvas_resized(mut self, f: impl FnMut(&mut S, Size, &mut Cx<'_>) + 'static) -> Self {
        self.resized = Some(Box::new(f));
        self
    }

    /// Runs when the page or window is hidden (`false`: a background tab, a minimized or covered
    /// window) or shown again (React Pong: `visibilitychange` pauses the game). Observers run
    /// after it.
    pub fn on_visibility_changed(mut self, f: impl FnMut(&mut S, bool, &mut Cx<'_>) + 'static) -> Self {
        self.visibility = Some(Box::new(f));
        self
    }

    /// Timing of the drawn frames (React Canvas.FrameTime / FPS).
    pub fn frame_stats(&self) -> FrameStats {
        self.tree.frame_stats
    }

    /// Tells the app whether its host keeps a browser history (`Cx::has_history`); the web host
    /// turns it on, a test plays the browser with it and `Headless::take_history`.
    pub fn set_history_enabled(&mut self, on: bool) {
        self.tree.history_enabled = on;
    }

    /// The canvas in points, as of the last frame.
    pub fn canvas_size(&self) -> Size {
        self.tree.canvas_size
    }

    /// Handlers and observers see this state; call after changing it from outside a handler.
    pub fn state_changed(&mut self) {
        self.state_dirty = true;
    }

    /// True when something changed since the last draw, or an animator is running.
    pub fn needs_frame(&self) -> bool {
        // Nothing is laid out, animated or routed before the fonts arrived: no frames while they
        // load (the host would draw empty frames back to back); the last one asks for the frame.
        if self.fonts_pending > 0 {
            return false;
        }
        let animating = animators::running(&self.tree);
        let long_press = self.recognizer.long_press_due().is_some_and(|due| due <= self.tree.time_ms);
        // A change under a hidden ancestor asks for no frame: nothing of it can be seen.
        let changed = self.tree.queue.iter().any(|&id| !self.tree.hidden_above(id));
        self.state_dirty || self.tree.needs_frame || animating || changed || !self.input.is_empty() || long_press
    }

    /// The frame time a sleeping animator, a timer, a pending long press or the accessibility
    /// snapshot wants the next frame at, when nothing else asks for one. The host draws a frame then.
    pub fn wake_at(&self) -> Option<f64> {
        // React OnFrameEnd: a frame inside the interval brings one more frame when it has passed.
        let accessibility = (self.accessibility_on && self.accessibility_stale).then(|| self.accessibility_built_ms + ACCESSIBILITY_INTERVAL_MS);
        [animators::next_wake(&self.tree), self.recognizer.long_press_due(), accessibility]
            .into_iter()
            .flatten()
            .filter(|t| *t > self.tree.time_ms)
            .min_by(f64::total_cmp)
    }

    /// Queues pointer input (pixels) of the left button; it is processed at the start of the next
    /// draw. `time_ms` is when it arrived, on the frame clock: velocities come from it, not from
    /// the frame it lands in.
    pub fn pointer(&mut self, kind: PointerKind, x: f32, y: f32, time_ms: f64) {
        self.pointer_button(kind, MouseButton::Left, x, y, time_ms);
    }

    /// Queues pointer input of any button. Hover moves are coalesced: only the last one before a
    /// frame is routed.
    pub fn pointer_button(&mut self, kind: PointerKind, button: MouseButton, x: f32, y: f32, time_ms: f64) {
        self.queue_pointer(kind, button, false, x, y, time_ms);
    }

    /// Queues the input of a finger (`Gesture::touch`), as `pointer`.
    pub fn pointer_touch(&mut self, kind: PointerKind, x: f32, y: f32, time_ms: f64) {
        self.queue_pointer(kind, MouseButton::Left, true, x, y, time_ms);
    }

    fn queue_pointer(&mut self, kind: PointerKind, button: MouseButton, touch: bool, x: f32, y: f32, time_ms: f64) {
        if kind == PointerKind::Down && std::mem::take(&mut self.focus_ring) {
            self.tree.needs_frame = true;
        }
        let event = (kind, button, touch, Point::new(x, y), time_ms);
        if kind == PointerKind::Hover
            && let Some(last) = self.input.back_mut().filter(|last| last.0 == PointerKind::Hover)
        {
            *last = event;
            return;
        }
        self.input.push_back(event);
    }

    /// Routes a vertical wheel event now, after the pointer input that is still queued: the host
    /// must know at once whether a control used it (DrawnUI LastInputUsed). `delta` is in notches,
    /// positive = up (toward the start).
    pub fn wheel(&mut self, x: f32, y: f32, delta: f32, time_ms: f64) -> bool {
        self.wheel_on_axis(x, y, delta, false, time_ms)
    }

    /// A horizontal wheel event (a tilted wheel, the sideways part of a touchpad swipe), as
    /// `wheel`; positive = left (toward the start). A vertical scroll leaves it.
    pub fn wheel_horizontal(&mut self, x: f32, y: f32, delta: f32, time_ms: f64) -> bool {
        self.wheel_on_axis(x, y, delta, true, time_ms)
    }

    fn wheel_on_axis(&mut self, x: f32, y: f32, delta: f32, horizontal: bool, time_ms: f64) -> bool {
        if self.fonts_pending > 0 || delta == 0.0 {
            return false;
        }
        self.flush_input();
        // Timers started by a wheel handler count from now, not from the last frame.
        self.tree.time_ms = self.tree.time_ms.max(time_ms);
        let mut gesture = plain_gesture(GestureKind::Wheel, Point::new(x, y), time_ms);
        (gesture.wheel, gesture.wheel_horizontal) = (delta, horizontal);
        // A layer that only keeps gestures from the controls below did not use the wheel.
        let consumed = self.process_gesture(gesture);
        self.used(consumed)
    }

    /// Routes a context menu request now (a right click, a long press on touch, the Menu key)
    /// over the pixel (x, y): the deepest control with a `on_context_menu` handler, then the
    /// `Ui::on_context_menu` fallback. True when one took it.
    pub fn context_menu(&mut self, x: f32, y: f32, source: ContextMenuSource, time_ms: f64) -> bool {
        if self.fonts_pending > 0 {
            return false;
        }
        self.flush_input();
        self.tree.time_ms = self.tree.time_ms.max(time_ms);
        let mut gesture = plain_gesture(GestureKind::ContextMenu, Point::new(x, y), time_ms);
        gesture.source = source;
        let consumed = self.process_gesture(gesture);
        if self.used(consumed) {
            return true;
        }
        let Some(fallback) = self.context_menu.as_mut() else { return false };
        let pixels = Point::new(x, y);
        let scale = self.scale.max(0.1);
        let menu = ContextMenu { location: Point::new(x / scale, y / scale), pixels, source, control: None, local: Point::default() };
        let taken = fallback(&mut self.state, &menu, &mut Cx { tree: &mut self.tree });
        self.state_dirty = true;
        self.apply_focus_request();
        taken
    }

    /// A control used the gesture: it consumed it and is not only a `block_gestures_below` layer.
    fn used(&self, consumed: Option<ControlId>) -> bool {
        consumed.is_some_and(|id| self.tree.base(id).is_some_and(|b| !b.p.block_gestures_below))
    }

    /// Routes a key event now, after the queued pointer input: the focused control first
    /// (`Control::on_key`, then its `on_key_down` / `on_key_up` / `on_key_char` handler), then the
    /// controls that listen to every key (`Cx::listen_keys`, the same two), then the window-level
    /// handlers. True when any of them used it.
    pub fn key(&mut self, kind: KeyKind, key: InputKey, text: &str, modifiers: Modifiers, repeat: bool) -> bool {
        if self.fonts_pending > 0 {
            return false;
        }
        self.flush_input();
        self.keyboard.apply(kind, key, modifiers);
        let Ui { tree, state, keyboard, key_down, key_up, key_char, state_dirty, .. } = self;
        let event = KeyEvent { kind, key, text, repeat, keyboard };
        let focused = tree.focused.filter(|f| tree.node(*f).is_some());
        let mut handled = focused.is_some_and(|f| key_to_control(tree, state, state_dirty, f, &event));
        // A key the focused control left goes to the control the accessibility overlay's focus
        // is on (C# SkiaAccessibilityManager: node.OnAccessibilityKey): a slider a keyboard user
        // tabbed to moves with the arrows.
        let overlay = tree.accessibility_focused.filter(|a| Some(*a) != focused && tree.node(*a).is_some());
        if let Some(overlay) = overlay.filter(|_| !handled) {
            handled = key_to_control(tree, state, state_dirty, overlay, &event);
        }
        // Listeners removed from the tree drop out here; one added by a handler waits for the next key.
        tree.key_listeners.retain(|id| tree.nodes.get(id.index as usize).is_some_and(|n| n.as_ref().is_some_and(|n| n.id == *id)));
        for i in 0..tree.key_listeners.len() {
            let Some(&id) = tree.key_listeners.get(i) else { break };
            if Some(id) != focused {
                handled |= key_to_control(tree, state, state_dirty, id, &event);
            }
        }
        let handlers = match kind {
            KeyKind::Down => key_down,
            KeyKind::Up => key_up,
            KeyKind::Char => key_char,
        };
        for handler in handlers.iter_mut() {
            handled |= handler(&mut *state, &event, &mut Cx { tree: &mut *tree });
            *state_dirty = true;
        }
        self.apply_focus_request();
        if !handled && kind == KeyKind::Down {
            if self.keyboard_navigation {
                handled = self.navigate(key, modifiers.shift);
            } else if self.accessibility_on && GROUP_KEYS.contains(&key) {
                // The browser's overlay keeps Tab, Enter and Escape; the groups are the engine's.
                handled = self.move_in_group(key);
            }
        }
        handled
    }

    /// Keyboard navigation for a host with none of its own (C# DrawnView.Windows OnCanvasKeyDown):
    /// Tab / Shift+Tab walk the tab stops in reading order, a group being one stop (its current
    /// item); arrows, Home and End walk a group; Enter and Space activate; Escape leaves.
    fn navigate(&mut self, key: InputKey, back: bool) -> bool {
        match key {
            "Tab" => self.tab(back),
            "Enter" | "NumpadEnter" | "Space" => match self.accessibility_focused() {
                Some(focused) => {
                    self.focus_ring = true;
                    self.accessibility_activate(focused.index);
                    true
                }
                None => false,
            },
            "Escape" => {
                let had = self.tree.accessibility_focused.take().is_some();
                self.focus_ring = false;
                self.tree.needs_frame = true;
                had
            }
            _ if GROUP_KEYS.contains(&key) => self.move_in_group(key),
            _ => false,
        }
    }

    /// The snapshot as it is now, built here when the last one is older than the frame (C#
    /// `Tab_WalksTheNewContent_WhenTheSnapshotRebuildWasSkipped`).
    fn fresh_accessibility(&mut self) {
        if self.accessibility_stale || self.tree.accessibility.is_empty() {
            let scale = self.scale.max(0.1);
            let size = self.tree.canvas_size;
            self.build_accessibility(size.width * scale, size.height * scale, scale, self.tree.time_ms);
        }
    }

    /// The nearest arrow-key group above a node of the snapshot (C# TryFindGroup).
    fn group_of(&self, control: ControlId) -> Option<ControlId> {
        let nodes = &self.tree.accessibility;
        let mut at = nodes.iter().find(|n| n.control == control)?.parent;
        while let Some(id) = at {
            let node = nodes.iter().find(|n| n.id == id)?;
            if GROUP_ROLES.contains(&node.role) {
                return Some(node.control);
            }
            at = node.parent;
        }
        None
    }

    /// The tab stops in reading order: every node that takes input, a group once (its current
    /// item, else its first one).
    fn tab_stops(&self) -> Vec<ControlId> {
        let mut stops = Vec::new();
        let mut groups = Vec::new();
        for n in self.tree.accessibility.iter().filter(|n| n.can_interact) {
            match self.group_of(n.control) {
                None => stops.push(n.control),
                Some(group) if !groups.contains(&group) => {
                    groups.push(group);
                    let current = self.group_items.iter().find(|(g, _)| *g == group).map(|(_, item)| *item);
                    let alive = current.filter(|c| self.tree.accessibility.iter().any(|m| m.control == *c && m.can_interact));
                    stops.push(alive.unwrap_or(n.control));
                }
                Some(_) => {}
            }
        }
        stops
    }

    fn tab(&mut self, back: bool) -> bool {
        self.fresh_accessibility();
        let stops = self.tab_stops();
        // After a click into an editor the keyboard goes on from it.
        let focused = self.accessibility_focused().or_else(|| self.focused());
        let group = focused.and_then(|f| self.group_of(f));
        let current = focused.and_then(|f| stops.iter().position(|s| *s == f || (group.is_some() && self.group_of(*s) == group)));
        let next = match (current, back) {
            (None, false) => (!stops.is_empty()).then_some(0),
            (None, true) => stops.len().checked_sub(1),
            (Some(i), false) => (i + 1 < stops.len()).then_some(i + 1),
            (Some(i), true) => i.checked_sub(1),
        };
        match next {
            Some(i) => self.keyboard_focus(stops[i]),
            // Past an end the focus leaves; the next Tab starts over.
            None => {
                self.tree.accessibility_focused = None;
                self.focus_ring = false;
                self.tree.needs_frame = true;
            }
        }
        true
    }

    /// Arrows, Home / End and PageUp / PageDown move between the items of a group by index (C#
    /// MoveInGroup): Down / Up in a column, Right / Left in a row, all four in a wrap, a grid or a
    /// split layout (Up / Down by one row), PageUp / PageDown by one viewport of the scroll around
    /// it. No wrap: at an end the key still belongs to the group. The target is the item when it
    /// takes input, else its first node that does; items that take none are skipped. Not ported:
    /// scrolling a recycled item that is not realized yet into view.
    fn move_in_group(&mut self, key: InputKey) -> bool {
        use crate::controls::layout::{LayoutType, SkiaLayout};
        #[derive(PartialEq)]
        enum Axis {
            Vertical,
            Horizontal,
            Both,
        }
        let Some(focused) = self.accessibility_focused() else { return false };
        self.fresh_accessibility();
        let Some(group) = self.group_of(focused) else { return false };
        let tree = &self.tree;
        let mut item = focused;
        while let Some(parent) = tree.parent(item).filter(|p| *p != group) {
            item = parent;
        }
        let items = tree.children(group).to_vec();
        let Some(current) = items.iter().position(|i| *i == item) else { return false };
        let layout = tree.find::<SkiaLayout>(group).map(|l| (l.p.layout_type, l.p.split));
        let axis = match layout {
            Some((LayoutType::Column, split)) if split <= 1 => Axis::Vertical,
            Some((LayoutType::Row, _)) => Axis::Horizontal,
            Some((LayoutType::Absolute, _)) | None => Axis::Vertical,
            Some(_) => Axis::Both,
        };
        // Items per row: the split, else the items on the first row. C# counts the row of the focused
        // item, which the last row of a wrap can leave short: Up from there skipped items.
        let row = match layout {
            _ if axis != Axis::Both => 1,
            Some((_, split)) if split > 1 => split as usize,
            _ => {
                let at = tree.base(items[0]).map(|b| b.rect).unwrap_or_default();
                let on_row = |i: &&ControlId| tree.base(**i).is_some_and(|b| b.p.is_visible && (b.rect.center_y() - at.center_y()).abs() < at.height() / 2.0);
                items.iter().filter(on_row).count().max(1)
            }
        };
        // Items in one viewport of the scroll around the group.
        let page = || {
            let mut scroll = tree.parent(group);
            while let Some(s) = scroll.filter(|s| tree.find::<crate::controls::scroll::SkiaScroll>(*s).is_none()) {
                scroll = tree.parent(s);
            }
            let (viewport, size) = (tree.base(scroll?)?.rect, tree.base(item)?.rect);
            let (viewport, size) = if axis == Axis::Horizontal { (viewport.width(), size.width()) } else { (viewport.height(), size.height()) };
            Some(if size > 0.0 { ((viewport / size) as usize).max(1) } else { 1 })
        };
        let (count, last) = (items.len(), items.len() - 1);
        let (target, step) = match key {
            "ArrowDown" if axis == Axis::Vertical => (current + 1, 1),
            "ArrowRight" if axis != Axis::Vertical => (current + 1, 1),
            "ArrowUp" if axis == Axis::Vertical => (current.wrapping_sub(1), -1),
            "ArrowLeft" if axis != Axis::Vertical => (current.wrapping_sub(1), -1),
            "ArrowDown" if axis == Axis::Both => ((current + row).min(last), 1),
            "ArrowUp" if axis == Axis::Both => (current.saturating_sub(row), -1),
            "Home" => (0, 1),
            "End" => (last, -1),
            "PageDown" => match page() {
                Some(page) => ((current + page * row).min(last), 1),
                None => return false,
            },
            "PageUp" => match page() {
                Some(page) => (current.saturating_sub(page * row), -1),
                None => return false,
            },
            _ => return false,
        };
        if target >= count || target == current {
            return true;
        }
        let mut at = target as isize;
        while let Some(&candidate) = usize::try_from(at).ok().and_then(|i| items.get(i)) {
            if let Some(node) = self.item_target(candidate) {
                self.keyboard_focus(node);
                return true;
            }
            at += step;
        }
        true
    }

    /// The node of a group item the keyboard goes to: the item when it takes input, else the
    /// first node inside it that does (C# FindTarget).
    fn item_target(&self, item: ControlId) -> Option<ControlId> {
        let inside = |mut at: ControlId| loop {
            if at == item {
                return true;
            }
            match self.tree.parent(at) {
                Some(parent) => at = parent,
                None => return false,
            }
        };
        let nodes = &self.tree.accessibility;
        let node = nodes.iter().find(|n| n.can_interact && n.control == item).or_else(|| nodes.iter().find(|n| n.can_interact && inside(n.control)));
        node.map(|n| n.control)
    }

    /// The keyboard puts the focus on a control: the scrolls above it bring it into view, a group
    /// remembers it, an editor takes the caret (and gives it up when the keyboard leaves), the ring
    /// shows.
    fn keyboard_focus(&mut self, control: ControlId) {
        if let Some(previous) = self.tree.accessibility_focused.filter(|p| *p != control) {
            self.accessibility_focus(previous.index, false);
        }
        self.accessibility_focus(control.index, true);
        if let Some(group) = self.group_of(control) {
            self.group_items.retain(|(g, _)| *g != group);
            self.group_items.push((group, control));
        }
        self.keyboard_moved = Some(control.index);
        self.focus_ring = true;
        self.tree.needs_frame = true;
    }

    /// The ring around the control the keyboard is on (C# DrawnView, 2 points of #6EA8FE, 2 points
    /// outside it, corners of 6), drawn over everything once the keyboard moved the focus.
    fn draw_focus_ring(&self, canvas: &Canvas, scale: f32) {
        // The browser draws its own ring around the overlay element.
        let Some(focused) = self.accessibility_focused().filter(|_| self.focus_ring && self.keyboard_navigation) else { return };
        // Not over a control that is not shown (C# draws it only where the control was drawn): a
        // page kept under the one pushed over it hides its controls.
        if !shown(&self.tree, focused) || !self.tree.accessibility.iter().any(|n| n.control == focused && n.can_interact) {
            return;
        }
        let Some(rect) = drawn_rect(&self.tree, focused) else { return };
        let (out, radius) = (2.0 * scale, 6.0 * scale);
        let ring = Rect::new(rect.left - out, rect.top - out, rect.right + out, rect.bottom + out);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_style(skia_safe::paint::Style::Stroke);
        paint.set_stroke_width(2.0 * scale);
        paint.set_color(Color::from_rgb(0x6E, 0xA8, 0xFE));
        canvas.draw_round_rect(ring, radius, radius, &paint);
    }

    /// The window lost the keyboard: no key is held anymore.
    pub fn blur(&mut self) {
        self.keyboard.reset();
    }

    /// The system's Back (Android): the innermost shown SkiaShell that can go back does (its top
    /// popup, else modal, else page: C# GoBack order). False when none can: the system has it.
    pub fn system_back(&mut self) -> bool {
        use crate::controls::shell::SkiaShell;
        let tree = &self.tree;
        let depth = |id: ControlId| std::iter::successors(tree.parent(id), |p| tree.parent(*p)).count();
        let shell = tree
            .nodes
            .iter()
            .flatten()
            .map(|n| n.id)
            .filter(|id| shown(tree, *id) && tree.find::<SkiaShell>(*id).is_some_and(|s| s.can_go_back()))
            .max_by_key(|id| depth(*id));
        let Some(shell) = shell else { return false };
        Cx { tree: &mut self.tree }.go_back(shell, true);
        self.tree.needs_frame = true;
        true
    }

    /// The host's focus went to something outside the canvas (a field of the page): no drawn
    /// control keeps the keyboard, so keys typed there never reach a drawn editor.
    pub fn focus_out(&mut self) {
        self.focus(None);
        self.tree.accessibility_focused = None;
        self.focus_ring = false;
        self.tree.needs_frame = true;
    }

    /// The control keys go to (DrawnUI FocusedChild).
    pub fn focused(&self) -> Option<ControlId> {
        self.tree.focused.filter(|f| self.tree.node(*f).is_some())
    }

    /// Moves the keyboard focus to a control (`None` clears it): the old and the new one get
    /// `on_focus_changed`, and the host's text input follows the new one.
    pub fn focus(&mut self, id: Option<ControlId>) {
        let id = id.filter(|id| self.tree.node(*id).is_some());
        let old = self.focused();
        if old == id {
            return;
        }
        self.tree.focused = id;
        if let Some(old) = old {
            self.fire_focus(old, false);
        }
        if let Some(new) = id {
            self.fire_focus(new, true);
        }
        self.update_text_input();
        self.tree.needs_frame = true;
    }

    fn fire_focus(&mut self, id: ControlId, focused: bool) {
        with_kind(&mut self.tree, id, |kind, cx| kind.on_focus_changed(cx, focused));
        if !self.tree.node(id).is_some_and(|n| n.handlers.input.as_ref().is_some_and(|h| h.focus_changed.is_some())) {
            return;
        }
        let Some(mut node) = self.tree.take(id) else { return };
        if let Some(control) = node.kind.as_deref_mut()
            && let Some(handler) = node.handlers.input.as_deref_mut().and_then(|h| h.focus_changed.as_mut())
        {
            let mut queue = Vec::new();
            let raw = Raw { id, control, base: &mut node.base, queue: &mut queue };
            handler(raw, &mut self.state, &mut Cx { tree: &mut self.tree }, focused);
            self.tree.queue.append(&mut queue);
            self.state_dirty = true;
        }
        self.tree.put_back(node);
    }

    /// Applies a focus claimed or given up by a control or a handler during routing.
    fn apply_focus_request(&mut self) {
        if let Some(request) = self.tree.focus_request.take() {
            self.focus(request);
        }
    }

    /// The host's text input follows the focused control: open over it when it takes typed text.
    fn update_text_input(&mut self) {
        let area = self.focused().and_then(|f| {
            let node = self.tree.node(f)?;
            node.kind.as_deref().is_some_and(|k| k.wants_text_input()).then(|| {
                let (r, s) = (node.base.rect, node.base.scale.max(0.1));
                Rect::new(r.left / s, r.top / s, r.right / s, r.bottom / s)
            })
        });
        if area != self.text_input {
            self.text_input = area;
            self.text_input_changed = true;
        }
    }

    /// The text input area the host was last told about (points), `None` = closed.
    pub fn text_input(&self) -> Option<Rect> {
        self.text_input
    }

    /// The mouse cursor the engine wants right now.
    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    /// The controls under the mouse, root first, as of the last hover (DrawnUI IsPointerOver).
    pub fn pointer_over(&self) -> &[ControlId] {
        &self.over
    }

    /// The controls under the mouse that take hover, root first (DrawnUI HoveredControls): each
    /// has `is_hovered`.
    pub fn hovered(&self) -> &[ControlId] {
        &self.hovered
    }

    /// The accessibility snapshot as last built.
    pub fn accessibility_nodes(&self) -> &[AccessibilityNode] {
        &self.tree.accessibility
    }

    /// The control the accessibility overlay's keyboard focus is on.
    pub fn accessibility_focused(&self) -> Option<ControlId> {
        self.tree.accessibility_focused.filter(|f| self.tree.node(*f).is_some())
    }

    /// Counts the snapshots that differed from the one before (React AccessibilityManager.Changed):
    /// a rebuild that finds nothing new does not move it.
    pub fn accessibility_revision(&self) -> u32 {
        self.accessibility_revision
    }

    /// The role every control of type `T` gets in the accessibility tree when it sets none
    /// (React `SkiaLabel.DefaultAccessibilityRole = Aria.RoleText`: every label is read).
    pub fn default_accessibility_role<T: Control>(mut self, role: &'static str) -> Self {
        let type_id = std::any::TypeId::of::<T>();
        self.accessibility_roles.retain(|(t, _)| *t != type_id);
        self.accessibility_roles.push((type_id, role));
        self
    }

    /// The overlay activated a control (a click on its node, Enter or Space): a Tapped at the
    /// center of its drawn bounds, to it and its subtree (React OnAccessibilityActivated). The
    /// focus follows the tap as for a real one: an editor that takes the tap takes the focus.
    pub fn accessibility_activate(&mut self, id: u32) {
        let Some(control) = self.tree.nodes.get(id as usize).and_then(|n| n.as_ref()).map(|n| n.id) else { return };
        let Some(rect) = self.tree.base(control).map(|b| b.rect) else { return };
        let scale = self.scale.max(0.1);
        let location = match self.tree.accessibility.iter().find(|n| n.control == control) {
            Some(n) => Point::new(n.rect.center_x() * scale, n.rect.center_y() * scale),
            None => Point::new(rect.center_x(), rect.center_y()),
        };
        let mut gesture = plain_gesture(GestureKind::Tapped, location, self.tree.time_ms);
        gesture.start = location;
        let point = to_local(&self.tree, control, location);
        let claimed_before = self.tree.focus_request.is_some();
        self.tree.gesture = Some(gesture);
        let routing = Routing { state: &mut self.state, state_dirty: &mut self.state_dirty, over: &mut self.over_next, by_z: &mut self.by_z };
        let consumed = Router { tree: &mut self.tree, r: routing }.route(control, &gesture, point);
        self.tree.gesture = None;
        self.tree.needs_frame = true;
        if !claimed_before && self.tree.focus_request.is_none() && consumed.is_some() {
            self.focus_after_tap(consumed);
        }
        self.apply_focus_request();
        self.accessibility_now();
    }

    /// The overlay's keyboard focus moved onto or off a control. Onto it: the scrolls above it
    /// bring it into view (React SkiaScroll.EnsureVisible). Keys its focused control does not use
    /// go to it (`Control::on_key`).
    /// A text field takes the caret when the focus comes onto it and gives it up when the focus
    /// goes elsewhere, also after a click into it (C# SkiaEditor.OnAccessibilityFocused).
    pub fn accessibility_focus(&mut self, id: u32, focused: bool) {
        let control = self.tree.nodes.get(id as usize).and_then(|n| n.as_ref()).map(|n| n.id);
        let text_input = |ui: &Self, id: ControlId| ui.tree.node(id).and_then(|n| n.kind.as_deref()).is_some_and(|k| k.wants_text_input());
        if focused {
            self.tree.accessibility_focused = control;
            let Some(control) = control else { return };
            ensure_visible(&mut self.tree, control);
            if text_input(self, control) {
                self.focus(Some(control));
            } else if self.focused().is_some_and(|f| text_input(self, f)) {
                self.focus(None);
            }
        } else if self.tree.accessibility_focused == control {
            self.tree.accessibility_focused = None;
            if control.is_some() && self.focused() == control && control.is_some_and(|c| text_input(self, c)) {
                self.focus(None);
            }
        }
    }

    /// A screen reader's Increment (`up`) or Decrement on a range control: ArrowUp / ArrowDown to
    /// it, as from the keyboard (a slider moves one step, `Control::key`).
    pub fn accessibility_adjust(&mut self, id: u32, up: bool) {
        let Some(control) = self.tree.nodes.get(id as usize).and_then(|n| n.as_ref()).map(|n| n.id) else { return };
        let Ui { tree, state, keyboard, state_dirty, .. } = self;
        let key = if up { "ArrowUp" } else { "ArrowDown" };
        let event = KeyEvent { kind: KeyKind::Down, key, text: "", repeat: false, keyboard };
        key_to_control(tree, state, state_dirty, control, &event);
        self.accessibility_now();
    }

    /// The snapshot is rebuilt at the end of the next frame, whatever the interval: a screen reader
    /// reads a value or a state back right after its action. (VoiceOver on macOS saw the old value
    /// for up to a second and fell back to SetValue in growing jumps, to the slider's ends.)
    fn accessibility_now(&mut self) {
        self.accessibility_built_ms = f64::NEG_INFINITY;
        self.tree.needs_frame = true;
    }

    /// A screen reader set a range control's value (`Control::accessibility_set_value`).
    pub fn accessibility_set_value(&mut self, id: u32, value: f64) {
        let Some(control) = self.tree.nodes.get(id as usize).and_then(|n| n.as_ref()).map(|n| n.id) else { return };
        with_kind(&mut self.tree, control, |kind, cx| kind.accessibility_set_value(cx, value));
        self.accessibility_now();
    }

    /// A screen reader paged a scroll's node (VoiceOver's three-finger swipe, TalkBack's scroll
    /// forward / back): `x` / `y` 1 shows what is right / below, -1 what is left / above, by the
    /// viewport less a tenth, animated as the keyboard focus scrolls.
    pub fn accessibility_scroll(&mut self, id: u32, x: f32, y: f32) {
        use crate::controls::scroll::SkiaScroll;
        let Some(control) = self.tree.nodes.get(id as usize).and_then(|n| n.as_ref()).map(|n| n.id) else { return };
        let (Some(scroll), Some(base)) = (self.tree.find::<SkiaScroll>(control), self.tree.base(control)) else { return };
        let page = base.rect.size() * (0.9 / base.scale.max(0.1));
        let (to_x, to_y) = (scroll.viewport_offset_x() - x * page.width, scroll.viewport_offset_y() - y * page.height);
        Cx { tree: &mut self.tree }.scroll_to(control, to_x, to_y, 250);
        self.tree.needs_frame = true;
    }

    /// A screen reader asked to bring a node on screen: the scrolls above it scroll to it.
    pub fn accessibility_scroll_into_view(&mut self, id: u32) {
        let Some(control) = self.tree.nodes.get(id as usize).and_then(|n| n.as_ref()).map(|n| n.id) else { return };
        ensure_visible(&mut self.tree, control);
        self.tree.needs_frame = true;
    }

    fn flush_input(&mut self) {
        while let Some((kind, button, touch, location, time_ms)) = self.input.pop_front() {
            match kind {
                // While content moves under the mouse, hover waits (one check when it stops).
                PointerKind::Hover if self.hover_paused => self.last_pointer = Some(location),
                PointerKind::Hover => self.hover(Some(location), time_ms),
                // Leaving the canvas ends hover at once, also while content moves.
                PointerKind::Leave => self.hover(None, time_ms),
                _ => {
                    for gesture in self.recognizer.feed(kind, button, touch, location, time_ms, self.scale).into_iter().flatten() {
                        self.process_gesture(gesture);
                    }
                }
            }
        }
        // The press was held long enough: fires once, between the frames' input.
        if let Some(gesture) = self.recognizer.long_press(self.tree.time_ms) {
            self.process_gesture(gesture);
        }
    }

    /// Routes a hover (`Some(location)`) or the mouse leaving (`None`): the controls under the
    /// pointer get `Pointer`, the ones newly under it `PointerEnter`, the ones no longer
    /// `PointerExit`; the cursor follows.
    fn hover(&mut self, location: Option<Point>, time_ms: f64) {
        self.last_pointer = location;
        self.tree.hover_check = false;
        self.over_next.clear();
        if let Some(location) = location {
            self.process_gesture(plain_gesture(GestureKind::Pointer, location, time_ms));
        }
        let Ui { tree, state, state_dirty, over, over_next, by_z, .. } = self;
        let mut router = Router { tree: &mut *tree, r: Routing { state, state_dirty, over: over_next, by_z } };
        for i in 0..router.r.over.len() {
            let id = router.r.over[i];
            if !over.contains(&id) {
                let local = location.map_or(Point::default(), |l| to_local(router.tree, id, l));
                router.deliver(id, &plain_gesture(GestureKind::PointerEnter, location.unwrap_or_default(), time_ms), local);
            }
        }
        for i in 0..over.len() {
            let id = over[i];
            if !router.r.over.contains(&id) {
                router.deliver(id, &plain_gesture(GestureKind::PointerExit, location.unwrap_or_default(), time_ms), Point::default());
            }
        }
        std::mem::swap(over, over_next);
        self.update_hovered();
        let Ui { tree, over, .. } = self;
        // The cursor: the deepest control with an answer, else the hand over anything tappable.
        let mut cursor = Cursor::Default;
        for &id in over.iter().rev() {
            let Some(node) = tree.node(id) else { continue };
            if let Some(c) = location.and_then(|l| node.kind.as_deref()?.cursor(to_local(tree, id, l))) {
                cursor = c;
                break;
            }
            let interactive = node.base.p.accessibility_can_interact.unwrap_or(node.handlers.tapped.is_some());
            if interactive {
                cursor = Cursor::Pointer;
                break;
            }
        }
        if cursor != self.cursor {
            self.cursor = cursor;
            self.cursor_changed = true;
        }
    }

    /// DrawnUI hover: after the pointer pass, the controls under the mouse that take hover are
    /// hovered, every one of them (a card and the button inside it alike, as CSS :hover); the ones
    /// that left get `on_hovered(false)`, the new ones `on_hovered(true)`.
    fn update_hovered(&mut self) {
        let Ui { tree, over, hovered_next, .. } = self;
        hovered_next.clear();
        for &id in over.iter() {
            let Some(node) = tree.node(id) else { continue };
            let takes = node.base.p.receives_hover.unwrap_or_else(|| node.kind.as_deref().is_some_and(|k| k.receives_hover()));
            if takes {
                hovered_next.push(id);
            }
        }
        for i in 0..self.hovered.len() {
            let id = self.hovered[i];
            if !self.hovered_next.contains(&id) {
                self.set_hovered(id, false);
            }
        }
        for i in 0..self.hovered_next.len() {
            let id = self.hovered_next[i];
            if !self.hovered.contains(&id) {
                self.set_hovered(id, true);
            }
        }
        std::mem::swap(&mut self.hovered, &mut self.hovered_next);
    }

    fn set_hovered(&mut self, id: ControlId, on: bool) {
        let Some(node) = self.tree.node_mut(id) else { return };
        if node.base.hovered == on {
            return;
        }
        node.base.hovered = on;
        if !node.handlers.input.as_ref().is_some_and(|h| h.hovered.is_some()) {
            return;
        }
        let Some(mut node) = self.tree.take(id) else { return };
        if let Some(control) = node.kind.as_deref_mut()
            && let Some(handler) = node.handlers.input.as_deref_mut().and_then(|h| h.hovered.as_mut())
        {
            let mut queue = Vec::new();
            let raw = Raw { id, control, base: &mut node.base, queue: &mut queue };
            handler(raw, &mut self.state, &mut Cx { tree: &mut self.tree }, on);
            self.tree.queue.append(&mut queue);
            self.state_dirty = true;
        }
        self.tree.put_back(node);
        self.tree.needs_frame = true;
    }

    /// The end of a frame: while a scroll, carousel or drawer moves its content, hover waits; when
    /// they stop, and after the controls under a still mouse may have changed (added, removed,
    /// rebound, hidden, scrolled by a jump), hover is checked again where the mouse is (DrawnUI
    /// OnHoverCheck).
    fn hover_after_frame(&mut self, time_ms: f64) {
        let tree = &self.tree;
        let moving = tree.movers.iter().any(|&id| {
            tree.node(id).is_some_and(|n| n.base.p.is_visible && n.kind.as_deref().and_then(|k| k.moves_content()) == Some(true))
        });
        if moving {
            self.hover_paused = true;
            return;
        }
        if std::mem::take(&mut self.hover_paused) {
            self.tree.hover_check = true;
        }
        // A hovered control (or an ancestor) was hidden under a still mouse: it stops being hovered.
        let tree = &self.tree;
        let shown = |mut id: Option<ControlId>| {
            while let Some(node) = id.and_then(|id| tree.node(id)) {
                if !node.base.p.is_visible {
                    return false;
                }
                id = node.parent;
            }
            true
        };
        if self.hovered.iter().any(|&id| !shown(Some(id))) {
            self.tree.hover_check = true;
        }
        if !std::mem::take(&mut self.tree.hover_check) || self.last_pointer.is_none() || self.recognizer.is_pressed() {
            return;
        }
        self.hover(self.last_pointer, time_ms);
    }

    /// A new FPS value goes into every SkiaLabelFps before the layout of the frame, so setting it
    /// asks for no frame of its own.
    fn update_fps_labels(&mut self) {
        if self.fps.value == self.fps.shown {
            return;
        }
        self.fps.shown = self.fps.value;
        // ponytail: a scan of the tree per new value (at most two a second while animating), with
        // no allocation unless a label is there; keep a list of the labels if a tree gets large.
        let mut text = None;
        for index in 0..self.tree.nodes.len() {
            let Some(id) = self.tree.nodes[index].as_ref().map(|n| n.id) else { continue };
            if let Some(mut label) = self.tree.find_mut::<SkiaLabelFps>(id) {
                label.set_text(text.get_or_insert_with(|| SkiaLabelFps::text(self.fps.value)).clone());
            }
        }
    }

    /// One frame, up to but not including the submit: input, animators, observers, invalidation,
    /// layout, paint. `time_ms` is the host's frame time (vsync-aligned where the host has one):
    /// animators run on it, never on a clock read here.
    /// Everything of a frame that does not draw: input, animators, observers, layout. A host whose
    /// drawing target makes it wait (Metal's next drawable, about a vsync) runs it before taking
    /// the target, so the wait does not add to the frame (drawnui-cross 6k); `draw` at the same
    /// time then only paints. `draw` runs it by itself otherwise.
    pub fn prepare(&mut self, width: f32, height: f32, scale: f32, time_ms: f64) {
        self.tree.needs_frame = false;
        self.tree.time_ms = time_ms;
        self.scale = scale;

        // Text sizes depend on the fonts: a layout made before they arrive would jump, and input
        // would land on controls that are about to move.
        if self.fonts_pending > 0 {
            self.input.clear();
            self.prepared = Some((time_ms, Rect::default(), std::time::Instant::now(), false));
            return;
        }

        let started = std::time::Instant::now();
        // Not fullscreen: the root's place is the canvas less the safe area.
        let inset = if self.tree.mobile_fullscreen { crate::types::Thickness::default() } else { self.tree.safe_insets };
        let content = Rect::from_ltrb(inset.left * scale, inset.top * scale, width - inset.right * scale, height - inset.bottom * scale);
        let canvas_size = Size::new(content.width().max(0.0) / scale.max(0.1), content.height().max(0.0) / scale.max(0.1));
        let insets_changed = std::mem::take(&mut self.insets_changed);
        if insets_changed {
            // Shells follow the safe area by themselves (their bars and pages, when fullscreen).
            for index in 0..self.tree.nodes.len() {
                let Some(id) = self.tree.nodes[index].as_ref().map(|n| n.id) else { continue };
                if self.tree.find::<crate::controls::shell::SkiaShell>(id).is_some() {
                    self.tree.invalidate(id, crate::types::Dirty::APPLY);
                }
            }
            if let Some(handler) = self.insets_handler.as_mut() {
                handler(&mut self.state, self.tree.safe_insets, &mut Cx { tree: &mut self.tree });
            }
            self.state_dirty = true;
        }
        if canvas_size != self.tree.canvas_size || insets_changed {
            self.tree.canvas_size = canvas_size;
            if let Some(resized) = self.resized.as_mut() {
                resized(&mut self.state, canvas_size, &mut Cx { tree: &mut self.tree });
            }
            self.state_dirty = true;
        }
        self.flush_input();
        if animators::tick(&mut self.tree, &mut self.state, time_ms) {
            self.state_dirty = true;
        }
        if self.state_dirty {
            self.state_dirty = false;
            self.tree.run_observers(&self.state);
        }
        self.apply_focus_request();
        self.update_fps_labels();
        layout::commit(&mut self.tree);

        let root = self.tree.root;
        if let Some(root) = root {
            layout::measure(&mut self.tree, &self.fonts, &self.state, root, content.width(), content.height(), scale);
            layout::arrange(&mut self.tree, &self.fonts, &self.state, root, content, scale);
            // Image handlers run before the bitmap is first painted.
            if images::dispatch(&mut self.tree, &mut self.state) {
                self.state_dirty = true;
            }
        }
        self.prepared = Some((time_ms, content, started, root.is_some()));
    }

    /// Draws a frame at `time_ms`: `prepare`, unless the host ran it for this time already, then
    /// the paint.
    pub fn draw(&mut self, canvas: &Canvas, gpu: &mut Gpu, width: f32, height: f32, scale: f32, time_ms: f64) {
        if self.prepared.is_none_or(|p| p.0 != time_ms) {
            self.prepare(width, height, scale, time_ms);
        }
        let Some((_, content, started, laid_out)) = self.prepared.take() else { return };
        // Caches replaced during the previous frame: that frame was submitted, they can go now.
        self.tree.drops.clear();
        // A new GPU context (the old one was lost): what lived on the old one is dead.
        if gpu.epoch() != self.tree.gpu_epoch {
            self.tree.gpu_epoch = gpu.epoch();
            paint::gpu_changed(&mut self.tree);
        }
        canvas.clear(self.background);
        let (Some(root), true) = (self.tree.root, laid_out) else { return };
        let full = content;

        let Tree { nodes, render, drops, animators, bakes, .. } = &mut self.tree;
        let fonts = &self.fonts;
        let mut cx = PaintCx { canvas, rect: full, scale, id: root, fonts, nodes, animators, render, drops, gpu, bakes, offthread: false, target: None };
        paint::render(&mut cx, root);
        // Shader files and textures paint found missing are fetched; arrived ones reach their effects.
        crate::effects::after_paint(&mut self.tree);
        // React OnCompilationError: shader effects that failed to compile or load during that paint.
        if crate::effects::dispatch_errors(&mut self.tree, &mut self.state) {
            self.state_dirty = true;
        }
        self.frame_times.push_back(time_ms);
        while self.frame_times.front().is_some_and(|t| *t <= time_ms - 1000.0) {
            self.frame_times.pop_front();
        }
        let frame_time_ms = started.elapsed().as_secs_f32() * 1000.0;
        self.tree.frame_stats = FrameStats { frame_time_ms, fps: self.frame_times.len() as u32 };
        self.draw_focus_ring(canvas, scale);
        self.fps.measure(time_ms);
        if self.fps.on {
            self.fps.draw(canvas, &self.fonts, content.bottom(), scale);
        }
        self.hover_after_frame(time_ms);
        self.fps.chained = self.needs_frame();
        // The focused control may have moved: the host's keyboard follows it.
        self.update_text_input();
        if self.accessibility_on {
            if time_ms - self.accessibility_built_ms >= ACCESSIBILITY_INTERVAL_MS {
                self.build_accessibility(width, height, scale, time_ms);
            } else {
                self.accessibility_stale = true;
            }
        }
    }

    /// Rebuilds the accessibility snapshot (React SkiaAccessibilityManager.OnFrameEnd): every
    /// visible control with a role, its drawn bounds in points, sorted top to bottom. Kept when
    /// nothing differs from the last one.
    fn build_accessibility(&mut self, width: f32, height: f32, scale: f32, time_ms: f64) {
        self.accessibility_built_ms = time_ms;
        self.accessibility_stale = false;
        let Some(root) = self.tree.root else { return };
        let mut list = std::mem::take(&mut self.accessibility_next);
        list.clear();
        let mut stack = std::mem::take(&mut self.accessibility_stack);
        stack.push((root, Matrix::new_identity(), None));
        while let Some((id, matrix, parent)) = stack.pop() {
            let mut below_parent = parent;
            let Some(node) = self.tree.node(id) else { continue };
            if !node.base.p.is_visible || node.base.p.opacity <= 0.0 {
                continue;
            }
            let matrix = match self.tree.render.get(id.index as usize).and_then(|r| r.matrix) {
                Some(own) => Matrix::concat(&matrix, &own),
                None => matrix,
            };
            let (p, kind) = (&node.base.p, node.kind.as_deref());
            // Set on the control, else the app's default for its type, else the control's own.
            let role = match p.accessibility_role {
                "" => kind.and_then(|k| type_role(&self.accessibility_roles, k).or_else(|| k.accessibility_role())).unwrap_or(""),
                role => role,
            };
            if !role.is_empty() && role != Aria::PRESENTATION {
                let rect = matrix.map_rect(node.base.rect).0;
                let outside = rect.right < -width || rect.bottom < -height || rect.left > 2.0 * width || rect.top > 2.0 * height;
                if rect.width() > 0.0 && rect.height() > 0.0 && !outside {
                    let label = match p.accessibility_label.as_str() {
                        "" => kind.and_then(|k| k.accessibility_label()).map(|l| l.into_owned()).unwrap_or_default(),
                        label => label.to_owned(),
                    };
                    let can_interact = p.accessibility_can_interact.or_else(|| kind.and_then(|k| k.accessibility_can_interact()));
                    let s = scale.max(0.1);
                    below_parent = Some(id.index);
                    list.push(AccessibilityNode {
                        id: id.index,
                        control: id,
                        role,
                        label,
                        hint: p.accessibility_hint.clone(),
                        rect: Rect::new(rect.left / s, rect.top / s, rect.right / s, rect.bottom / s),
                        can_interact: can_interact.unwrap_or(node.handlers.tapped.is_some()),
                        is_pressed: p.accessibility_is_pressed.or_else(|| kind.and_then(|k| k.accessibility_is_pressed())),
                        value: kind.and_then(|k| k.accessibility_value()),
                        scrolls: self.tree.find::<crate::controls::scroll::SkiaScroll>(id).map_or((false, false), |s| s.scroll_axes()),
                        live: p.accessibility_live,
                        text_lines: match (p.accessibility_text_selectable, kind) {
                            (true, Some(k)) => k.accessibility_text_lines(scale),
                            _ => Vec::new(),
                        },
                        parent,
                        group: None,
                    });
                }
            }
            let mut below = matrix;
            below.pre_translate(node.base.content_offset);
            // Reversed: children come off the stack in tree order, which equal positions keep.
            stack.extend(node.children.iter().rev().map(|c| (*c, below, below_parent)));
        }
        list.sort_by(|a, b| a.rect.top.total_cmp(&b.rect.top).then(a.rect.left.total_cmp(&b.rect.left)));
        // Reading order (C# SkiaAccessibilityManager): nodes whose tops are within half the smaller
        // height of the row's first one form a row, read left to right, so a row of vertically
        // centered controls of different heights keeps its visual order.
        let mut row = 0;
        for i in 1..=list.len() {
            if let Some(next) = list.get(i).map(|n| n.rect) {
                let first = list[row].rect;
                if next.top - first.top < first.height().min(next.height()) / 2.0 {
                    continue;
                }
            }
            list[row..i].sort_by(|a, b| a.rect.left.total_cmp(&b.rect.left));
            row = i;
        }
        // The arrow-key group of each node and the group's item that holds it.
        for i in 0..list.len() {
            let (mut at, mut group) = (list[i].parent, None);
            while let Some(id) = at {
                let Some(node) = list.iter().find(|n| n.id == id) else { break };
                if GROUP_ROLES.contains(&node.role) {
                    group = Some(node.control);
                    break;
                }
                at = node.parent;
            }
            list[i].group = group.and_then(|group| {
                let mut item = list[i].control;
                while let Some(parent) = self.tree.parent(item) {
                    if parent == group {
                        return Some((group.index, item.index));
                    }
                    item = parent;
                }
                None
            });
        }
        let same = list.len() == self.tree.accessibility.len()
            && list.iter().zip(&self.tree.accessibility).all(|(a, b)| {
                a.control == b.control
                    && a.role == b.role
                    && a.label == b.label
                    && a.hint == b.hint
                    && a.can_interact == b.can_interact
                    && a.is_pressed == b.is_pressed
                    && a.value == b.value
                    && a.scrolls == b.scrolls
                    && a.live == b.live
                    && a.parent == b.parent
                    && a.group == b.group
                    && a.text_lines.len() == b.text_lines.len()
                    && a.text_lines.iter().zip(&b.text_lines).all(|(x, y)| {
                        x.text == y.text
                            && (x.left - y.left).abs() <= 0.5
                            && (x.top - y.top).abs() <= 0.5
                            && x.font_size == y.font_size
                    })
                    && (a.rect.left - b.rect.left).abs() <= 0.5
                    && (a.rect.top - b.rect.top).abs() <= 0.5
                    && (a.rect.right - b.rect.right).abs() <= 0.5
                    && (a.rect.bottom - b.rect.bottom).abs() <= 0.5
            });
        self.accessibility_stack = stack;
        if same {
            self.accessibility_next = list;
        } else {
            self.accessibility_next = std::mem::replace(&mut self.tree.accessibility, list);
            self.accessibility_changed = true;
            self.accessibility_revision += 1;
        }
    }

    /// Tab, arrows in groups, Enter / Space, Escape and the focus ring for a host with no keyboard
    /// navigation of its own (the desktop host turns it on; off by default).
    pub fn set_keyboard_navigation(&mut self, on: bool) {
        self.keyboard_navigation = on;
    }

    /// Builds the accessibility snapshot (at most once per `ACCESSIBILITY_INTERVAL_MS`, at the
    /// end of a frame) for a host that renders it. The web host turns it on; off by default.
    pub fn set_accessibility_enabled(&mut self, on: bool) {
        self.accessibility_on = on;
        if on {
            self.accessibility_built_ms = f64::NEG_INFINITY;
            self.tree.needs_frame = true;
        }
    }

    /// Routes one gesture into the tree. Returns the control that consumed it. Handlers read the
    /// gesture with `Cx::gesture` (the button of a tap).
    pub fn process_gesture(&mut self, gesture: Gesture) -> Option<ControlId> {
        self.tree.gesture = Some(gesture);
        let consumed = self.route_gesture(gesture);
        self.tree.gesture = None;
        consumed
    }

    fn route_gesture(&mut self, gesture: Gesture) -> Option<ControlId> {
        let root = self.tree.root?;
        let claimed_before = self.tree.focus_request.is_some();
        let routing = Routing { state: &mut self.state, state_dirty: &mut self.state_dirty, over: &mut self.over_next, by_z: &mut self.by_z };
        let mut router = Router { tree: &mut self.tree, r: routing };
        if gesture.kind == GestureKind::Down {
            self.owner = None;
        } else if let Some(owner) = self.owner
            // The gestures replayed to the owner first (React / C# IsSavedGesture).
            && matches!(gesture.kind, GestureKind::Panning | GestureKind::Up | GestureKind::Wheel)
        {
            let alive = router.tree.node(owner).is_some_and(|n| n.base.p.is_visible && !n.base.p.input_transparent);
            let consumed = if alive { router.route(owner, &gesture, to_local(router.tree, owner, gesture.location)) } else { None };
            if consumed.is_some() {
                // It still wants the gesture: nobody else sees this one.
                self.owner = if gesture.kind == GestureKind::Up { None } else { consumed };
                self.apply_focus_request();
                return consumed;
            }
            // It let go (a button whose press turned into a pan). A wheel it did not use leaves the
            // press with it.
            if gesture.kind != GestureKind::Wheel {
                self.owner = None;
            }
        }
        let point = to_local(router.tree, root, gesture.location);
        let consumed = router.route(root, &gesture, point);
        // Only the press has an owner: a wheel between presses, a hover, a long press and the
        // context menu go to what is under the pointer every time.
        match gesture.kind {
            GestureKind::Down | GestureKind::Panning => self.owner = consumed,
            GestureKind::Up => self.owner = None,
            _ => {}
        }
        if gesture.kind == GestureKind::Tapped && !claimed_before && self.tree.focus_request.is_none() {
            self.focus_after_tap(consumed);
        }
        self.apply_focus_request();
        consumed
    }

    /// DrawnUI's focus rule: the canvas decides on the completed tap only. The consumer takes the
    /// focus when it can be focused; a tap over nothing clears it; a tap on a control that does
    /// not take focus leaves it where it is; a locked focus stays in every case. Skipped when a
    /// control claimed the focus during this gesture (an editor on its Down).
    fn focus_after_tap(&mut self, consumed: Option<ControlId>) {
        let focused = self.focused();
        if focused.is_some_and(|f| self.tree.base(f).is_some_and(|b| b.p.lock_focus)) {
            return;
        }
        match consumed {
            Some(c) if Some(c) == focused => {}
            Some(c) => {
                if self.tree.base(c).is_some_and(|b| b.p.can_be_focused) {
                    self.focus(Some(c));
                }
            }
            None => self.focus(None),
        }
    }
}

/// The keys a group takes: arrows, Home / End, PageUp / PageDown (C# SkiaAccessibilityManager.Key).
const GROUP_KEYS: [&str; 8] = ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End", "PageUp", "PageDown"];

/// Roles whose items are one Tab stop, walked with the arrows (C# Aria.IsCompositeRole).
const GROUP_ROLES: [&str; 8] = [Aria::LIST, Aria::LIST_BOX, Aria::GRID, Aria::TOOLBAR, Aria::RADIO_GROUP, Aria::TAB_LIST, Aria::MENU, Aria::MENU_BAR];

/// The control and every control above it are visible (C# IsVisible, IsHiddenByAncestor).
fn shown(tree: &Tree, id: ControlId) -> bool {
    let mut at = Some(id);
    while let Some(control) = at {
        match tree.base(control) {
            Some(base) if base.p.is_visible && base.p.opacity > 0.0 => at = tree.parent(control),
            _ => return false,
        }
    }
    true
}

/// Where a control is drawn on the canvas, pixels: its rect through every render matrix and
/// scroll offset above it, as the accessibility snapshot maps it.
fn drawn_rect(tree: &Tree, id: ControlId) -> Option<Rect> {
    let mut path = vec![id];
    while let Some(parent) = tree.parent(*path.last()?) {
        path.push(parent);
    }
    path.reverse();
    let mut matrix = Matrix::new_identity();
    for (i, &at) in path.iter().enumerate() {
        if let Some(own) = tree.render.get(at.index as usize).and_then(|r| r.matrix) {
            matrix = Matrix::concat(&matrix, &own);
        }
        if i + 1 < path.len() {
            matrix.pre_translate(tree.base(at)?.content_offset);
        }
    }
    Some(matrix.map_rect(tree.base(id)?.rect).0)
}

/// React SkiaScroll.EnsureVisible: every scroll above the control scrolls, over 250 ms, just far
/// enough to show it with 8 points to spare.
fn ensure_visible(tree: &mut Tree, id: ControlId) {
    use crate::controls::scroll::SkiaScroll;
    const PADDING: f32 = 8.0;
    const MS: f32 = 250.0;
    let Some(mut rect) = tree.base(id).map(|b| b.rect) else { return };
    let mut child = id;
    while let Some(parent) = tree.parent(child) {
        // Where the control is drawn: children move by their parents' content offsets.
        let Some(base) = tree.base(parent) else { return };
        rect.offset(base.content_offset);
        let (viewport, scale) = (base.rect, base.scale.max(0.1));
        if let Some(scroll) = tree.find::<SkiaScroll>(parent) {
            let (x, y) = (scroll.viewport_offset_x(), scroll.viewport_offset_y());
            let mut delta = Point::default();
            if rect.top < viewport.top {
                delta.y = (viewport.top - rect.top) / scale + PADDING;
            } else if rect.bottom > viewport.bottom {
                delta.y = -((rect.bottom - viewport.bottom) / scale + PADDING);
            }
            if rect.left < viewport.left {
                delta.x = (viewport.left - rect.left) / scale + PADDING;
            } else if rect.right > viewport.right {
                delta.x = -((rect.right - viewport.right) / scale + PADDING);
            }
            if !delta.is_zero() {
                Cx { tree: &mut *tree }.scroll_to(parent, x + delta.x, y + delta.y, MS);
            }
        }
        child = parent;
    }
}

/// The role the app gave the control's type or one of its base parts (`default_accessibility_role`).
fn type_role(roles: &[(std::any::TypeId, &'static str)], control: &dyn Control) -> Option<&'static str> {
    let mut part = Some(control);
    while let Some(c) = part {
        let type_id = (c as &dyn Any).type_id();
        if let Some((_, role)) = roles.iter().find(|(t, _)| *t == type_id) {
            return Some(role);
        }
        part = c.inner();
    }
    None
}

/// Runs a hook of the control with only its kind out of the node: the base stays readable and
/// invalidation reaches it, as in the gesture router. `None` when the control is not mounted.
fn with_kind<R>(tree: &mut Tree, id: ControlId, run: impl FnOnce(&mut dyn Control, &mut GestureCx) -> R) -> Option<R> {
    let mut kind = tree.node_mut(id)?.kind.take()?;
    let result = run(&mut *kind, &mut GestureCx::new(&mut *tree, id, Point::default()));
    if let Some(node) = tree.node_mut(id) {
        node.kind = Some(kind);
    }
    Some(result)
}

/// A key to one control: its `on_key` hook, then its handler for the kind. True when used.
fn key_to_control(tree: &mut Tree, state: &mut dyn Any, state_dirty: &mut bool, id: ControlId, event: &KeyEvent<'_>) -> bool {
    let Some(mut handled) = with_kind(tree, id, |kind, cx| kind.on_key(cx, event)) else { return false };
    tree.needs_frame = true;
    let Some(mut node) = tree.take(id) else { return handled };
    if let Some(control) = node.kind.as_deref_mut() {
        let handler = node.handlers.input.as_deref_mut().and_then(|h| match event.kind {
            KeyKind::Down => h.key_down.as_mut(),
            KeyKind::Up => h.key_up.as_mut(),
            KeyKind::Char => h.key_char.as_mut(),
        });
        if let Some(handler) = handler {
            let mut queue = Vec::new();
            let raw = Raw { id, control, base: &mut node.base, queue: &mut queue };
            handled |= handler(raw, state, &mut Cx { tree: &mut *tree }, event);
            tree.queue.append(&mut queue);
            *state_dirty = true;
        }
    }
    tree.put_back(node);
    tree.needs_frame = true;
    handled
}

/// A gesture with no movement and no button: a wheel, a hover, a context menu.
fn plain_gesture(kind: GestureKind, location: Point, time_ms: f64) -> Gesture {
    let zero = Point::default();
    let (start, delta, total, velocity, wheel, cancelled) = (location, zero, zero, zero, 0.0, false);
    let (button, source) = (MouseButton::Left, ContextMenuSource::Mouse);
    let (wheel_horizontal, touch) = (false, false);
    Gesture { kind, location, start, delta, total, time_ms, velocity, wheel, wheel_horizontal, cancelled, button, source, touch }
}

/// Maps a canvas point into the control's own space, through every ancestor's render transform.
fn to_local(tree: &Tree, id: ControlId, point: Point) -> Point {
    let Some(parent) = tree.parent(id) else { return map_into(tree, id, point) };
    let offset = tree.base(parent).map_or(Point::default(), |b| b.content_offset);
    map_into(tree, id, to_local(tree, parent, point) - offset)
}

/// Maps a point from the parent's space into the control's own (untransformed) space.
fn map_into(tree: &Tree, id: ControlId, point: Point) -> Point {
    match tree.render[id.index as usize].matrix.and_then(|m| m.invert()) {
        Some(inverse) => inverse.map_point(point),
        None => point,
    }
}

/// What the router carries besides the tree. `GestureCx` holds it, so a control can route the
/// gesture into its children from its own hook (`GestureCx::route_children`).
pub(crate) struct Routing<'a> {
    state: &'a mut dyn Any,
    state_dirty: &'a mut bool,
    /// Every control a `Pointer` gesture went through, root first.
    over: &'a mut Vec<ControlId>,
    /// Children sorted by z order: one segment per control being routed, the nested ones after
    /// it; kept for its allocation.
    by_z: &'a mut Vec<ControlId>,
}

impl Routing<'_> {
    fn reborrow(&mut self) -> Routing<'_> {
        Routing { state: &mut *self.state, state_dirty: &mut *self.state_dirty, over: &mut *self.over, by_z: &mut *self.by_z }
    }
}

/// `GestureCx::route_children`: the children pass of `Router::route` for `id` at `point`.
pub(crate) fn route_children(tree: &mut Tree, routing: &mut Routing<'_>, id: ControlId, gesture: &Gesture, point: Point) -> Option<ControlId> {
    Router { tree, r: routing.reborrow() }.route_children(id, gesture, point)
}

struct Router<'a> {
    tree: &'a mut Tree,
    r: Routing<'a>,
}

impl Router<'_> {
    /// Same order as DrawnUI ProcessGestures: the app's `consume_gestures` handler, the effects,
    /// the control's own hook, its children top-most first, then its tapped handler. `point` is
    /// in the control's own space.
    fn route(&mut self, id: ControlId, gesture: &Gesture, point: Point) -> Option<ControlId> {
        let node = self.tree.node(id)?;
        let (block, consumes) = (node.base.p.block_gestures_below, node.handlers.input.as_ref().is_some_and(|h| h.consume_gestures.is_some()));
        if gesture.kind == GestureKind::Pointer {
            self.r.over.push(id);
        }
        // React / C# ConsumeGestures: the app decides before the control and its children.
        if consumes && self.fire_consume_gestures(id, gesture, point) {
            return Some(id);
        }
        // Effects that take gestures (a ripple, a shader) see it before the control does (C#
        // EffectsGestureProcessors; React SkiaControl.ProcessGestures, before the children).
        if crate::effects::route(self.tree, id, gesture, point) {
            return Some(id);
        }

        let mut children_routed = false;
        if let Some(mut kind) = self.tree.node_mut(id).and_then(|n| n.kind.take()) {
            let mut cx = GestureCx { tree: &mut *self.tree, id, point, routing: Some(self.r.reborrow()), children_routed: false };
            let handled = kind.on_gesture(&mut cx, gesture);
            children_routed = cx.children_routed;
            if let Some(node) = self.tree.node_mut(id) {
                node.kind = Some(kind);
            }
            match handled {
                Handled::Yes => return Some(id),
                Handled::Tapped => {
                    self.fire_tapped(id, point);
                    return Some(id);
                }
                Handled::By(child) => return Some(child),
                Handled::No => {}
            }
        }

        if !children_routed && let Some(consumed) = self.route_children(id, gesture, point) {
            return Some(consumed);
        }

        match gesture.kind {
            GestureKind::Tapped if self.fire_tapped(id, point) => return Some(id),
            GestureKind::LongPressing if self.fire_long_pressing(id, point) => return Some(id),
            GestureKind::ContextMenu if self.fire_context_menu(id, gesture, point) => return Some(id),
            _ => {}
        }
        block.then_some(id)
    }

    /// The children part of `route`: none while `lock_children_gestures` keeps them from this
    /// gesture, else each child under the point, top-most first, until one consumes it.
    fn route_children(&mut self, id: ControlId, gesture: &Gesture, point: Point) -> Option<ControlId> {
        let base = self.tree.base(id)?;
        let locked = match base.p.lock_children_gestures {
            LockTouch::Disabled => false,
            LockTouch::Enabled | LockTouch::PassNone => true,
            LockTouch::PassTap => gesture.kind != GestureKind::Tapped,
            LockTouch::PassTapAndLongPress => !matches!(gesture.kind, GestureKind::Tapped | GestureKind::LongPressing),
        };
        if locked {
            return None;
        }
        // Children are drawn moved by the content offset (scrolling).
        let point = point - base.content_offset;
        let tree = &*self.tree;
        if tree.children(id).iter().any(|c| tree.node(*c).is_some_and(|n| n.base.p.z_index != 0)) {
            // Higher z first, equal z by index: a stable sort of this control's segment.
            let start = self.r.by_z.len();
            self.r.by_z.extend_from_slice(tree.children(id));
            self.r.by_z[start..].sort_by_key(|c| tree.node(*c).map_or(0, |n| n.base.p.z_index));
            let mut consumed = None;
            for i in (start..self.r.by_z.len()).rev() {
                consumed = self.route_child(self.r.by_z[i], gesture, point);
                if consumed.is_some() {
                    break;
                }
            }
            // Nested routes pushed after the segment and truncated back to it.
            self.r.by_z.truncate(start);
            return consumed;
        }
        // Top-most last child first, by index: a handler may change the list, so it is looked up
        // again every step and never copied.
        let mut i = self.tree.children(id).len();
        while i > 0 {
            i -= 1;
            let Some(&child) = self.tree.children(id).get(i) else { continue };
            if let Some(consumed) = self.route_child(child, gesture, point) {
                return Some(consumed);
            }
        }
        None
    }

    /// Routes into one child when the point (the parent's space, offset applied) is inside it.
    fn route_child(&mut self, child: ControlId, gesture: &Gesture, point: Point) -> Option<ControlId> {
        let base = self.tree.base(child)?;
        if !base.p.is_visible || base.p.input_transparent {
            return None;
        }
        let local = map_into(self.tree, child, point);
        let r = base.rect;
        if local.x < r.left || local.x >= r.right || local.y < r.top || local.y >= r.bottom {
            return None;
        }
        self.route(child, gesture, local)
    }

    /// Hands a gesture to one control's hook alone (PointerEnter / PointerExit).
    fn deliver(&mut self, id: ControlId, gesture: &Gesture, point: Point) {
        let Some(mut kind) = self.tree.node_mut(id).and_then(|n| n.kind.take()) else { return };
        kind.on_gesture(&mut GestureCx::new(&mut *self.tree, id, point), gesture);
        if let Some(node) = self.tree.node_mut(id) {
            node.kind = Some(kind);
        }
    }

    /// Runs a handler of the control with the node out of its slot, so the handler can reach the
    /// rest of the tree; its own control is `me`. False when the control has no such handler.
    /// `point` is where the gesture is in the control's space (pixels): `Cx::gesture_point` gives
    /// it in points from the control's top-left while the handler runs.
    fn fire(
        &mut self,
        id: ControlId,
        point: Point,
        run: impl FnOnce(Raw<'_>, &mut dyn Any, &mut Cx<'_>, &mut crate::tree::Handlers) -> bool,
    ) -> bool {
        let Some(mut node) = self.tree.take(id) else { return false };
        let mut fired = false;
        let mut queue = Vec::new();
        if let Some(control) = node.kind.as_deref_mut() {
            let (rect, scale) = (node.base.rect, node.base.scale.max(0.1));
            self.tree.gesture_point = Some(Point::new((point.x - rect.left) / scale, (point.y - rect.top) / scale));
            let raw = Raw { id, control, base: &mut node.base, queue: &mut queue };
            fired = run(raw, &mut *self.r.state, &mut Cx { tree: self.tree }, &mut node.handlers);
            self.tree.gesture_point = None;
            if fired {
                *self.r.state_dirty = true;
            }
        }
        self.tree.put_back(node);
        self.tree.queue.append(&mut queue);
        fired
    }

    /// Runs the control's tapped handler, after its `animation_tapped` feedback at `point` (the
    /// control's space, pixels), as DrawnUI SendTapped. False when it has no handler.
    fn fire_tapped(&mut self, id: ControlId, point: Point) -> bool {
        self.fire(id, point, |raw, state, cx, handlers| match handlers.tapped.as_mut() {
            Some(tapped) => {
                let (p, rect, scale) = (&raw.base.p, raw.base.rect, raw.base.scale.max(0.1));
                if p.animation_tapped == crate::controls::button::SkiaTouchAnimation::Ripple {
                    let (x, y) = ((point.x - rect.left) / scale, (point.y - rect.top) / scale);
                    cx.play_ripple(id, p.touch_effect_color, x, y, p.animation_tapped_speed);
                }
                tapped(raw, state, cx);
                true
            }
            None => false,
        })
    }

    /// Runs the control's `consume_gestures` handler; true when it consumed the gesture.
    fn fire_consume_gestures(&mut self, id: ControlId, gesture: &Gesture, point: Point) -> bool {
        let mut consumed = false;
        self.fire(id, point, |raw, state, cx, handlers| match handlers.input.as_deref_mut().and_then(|h| h.consume_gestures.as_mut()) {
            Some(handler) => {
                consumed = handler(raw, state, cx, gesture);
                true
            }
            None => false,
        });
        consumed
    }

    fn fire_long_pressing(&mut self, id: ControlId, point: Point) -> bool {
        self.fire(id, point, |raw, state, cx, handlers| match handlers.input.as_deref_mut().and_then(|h| h.long_pressing.as_mut()) {
            Some(long_pressing) => {
                long_pressing(raw, state, cx);
                true
            }
            None => false,
        })
    }

    /// Runs the control's context menu handler; true when it took the request.
    fn fire_context_menu(&mut self, id: ControlId, gesture: &Gesture, point: Point) -> bool {
        let Some(base) = self.tree.base(id) else { return false };
        let scale = base.scale.max(0.1);
        let pixels = gesture.location;
        let menu = ContextMenu {
            location: Point::new(pixels.x / scale, pixels.y / scale),
            pixels,
            source: gesture.source,
            control: Some(id),
            local: Point::new(point.x - base.rect.left, point.y - base.rect.top),
        };
        self.fire(id, point, |raw, state, cx, handlers| match handlers.input.as_deref_mut().and_then(|h| h.context_menu.as_mut()) {
            Some(context_menu) => context_menu(raw, state, cx, &menu),
            None => false,
        })
    }
}

// ---------------------------------------------------------------- builder handlers

impl<T: Control> Build<T> {
    fn input(&mut self) -> &mut InputHandlers {
        self.handlers.input.get_or_insert_with(Default::default)
    }

    /// The control gets every key while it is mounted, focused or not (React
    /// KeyboardManager.Subscribe: a game, a page's shortcuts): its `Control::on_key` and its key
    /// handlers, after the focused control. `Cx::listen_keys` turns it on or off later.
    pub fn listen_keys(mut self) -> Self {
        self.input().listen_keys = true;
        self
    }

    /// The control gets `Control::on_history` while it is mounted: the browser went back or
    /// forward (a shell with browser history). `Cx::listen_history` turns it on or off later.
    pub fn listen_history(mut self) -> Self {
        self.input().listen_history = true;
        self
    }

    /// Sees every gesture that lands on the control before the control and its children (React /
    /// C# ConsumeGestures; a ProcessGestures override in code-behind): a drag handle, a game.
    /// True = consumed: the control becomes the owner of the press (Panning, Up and the wheel
    /// come to it first), nothing below sees it. `gesture.location` is canvas pixels;
    /// `cx.gesture_point()` is where it is in points inside the control, `gesture.velocity /
    /// me.base().scale` its speed in points per second. Changed from React: an Up it answers true to is consumed too
    /// (React always routes the Up on, and the owner's handler sees it twice).
    pub fn consume_gestures<S: Any>(
        mut self,
        mut f: impl FnMut(&mut crate::Mut<'_, T>, &mut S, &mut Cx<'_>, &Gesture) -> bool + 'static,
    ) -> Self {
        self.input().consume_gestures = Some(Box::new(move |raw, state, cx, gesture| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| crate::tree::wrong_state::<S>());
            f(&mut raw.typed(), state, cx, gesture)
        }));
        self
    }

    /// The press stayed on the control for `LONG_PRESS_MS` without moving (DrawnUI LongPressing).
    pub fn on_long_pressing<S: Any>(mut self, mut f: impl FnMut(&mut crate::Mut<'_, T>, &mut S, &mut Cx<'_>) + 'static) -> Self {
        self.input().long_pressing = Some(Box::new(move |raw, state, cx| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| crate::tree::wrong_state::<S>());
            f(&mut raw.typed(), state, cx)
        }));
        self
    }

    /// A context menu request over the control (React ContextMenu): a right click, a long press
    /// on touch, the Menu key. Return true to take it: the browser's own menu stays away. Asked
    /// deepest control first.
    pub fn on_context_menu<S: Any>(mut self, mut f: impl FnMut(&mut crate::Mut<'_, T>, &mut S, &mut Cx<'_>, &ContextMenu) -> bool + 'static) -> Self {
        self.input().context_menu = Some(Box::new(move |raw, state, cx, menu| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| crate::tree::wrong_state::<S>());
            f(&mut raw.typed(), state, cx, menu)
        }));
        self
    }

    /// A key went down while the control is focused. True = used.
    pub fn on_key_down<S: Any>(mut self, mut f: impl FnMut(&mut crate::Mut<'_, T>, &mut S, &mut Cx<'_>, &KeyEvent<'_>) -> bool + 'static) -> Self {
        self.input().key_down = Some(Box::new(move |raw, state, cx, event| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| crate::tree::wrong_state::<S>());
            f(&mut raw.typed(), state, cx, event)
        }));
        self
    }

    /// A key went up while the control is focused. True = used.
    pub fn on_key_up<S: Any>(mut self, mut f: impl FnMut(&mut crate::Mut<'_, T>, &mut S, &mut Cx<'_>, &KeyEvent<'_>) -> bool + 'static) -> Self {
        self.input().key_up = Some(Box::new(move |raw, state, cx, event| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| crate::tree::wrong_state::<S>());
            f(&mut raw.typed(), state, cx, event)
        }));
        self
    }

    /// Text was typed while the control is focused (`event.text`). True = used.
    pub fn on_key_char<S: Any>(mut self, mut f: impl FnMut(&mut crate::Mut<'_, T>, &mut S, &mut Cx<'_>, &KeyEvent<'_>) -> bool + 'static) -> Self {
        self.input().key_char = Some(Box::new(move |raw, state, cx, event| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| crate::tree::wrong_state::<S>());
            f(&mut raw.typed(), state, cx, event)
        }));
        self
    }

    /// The mouse came over the control (true) or left it (false); the control takes hover then
    /// (`receives_hover`, set for it unless its type takes hover by default). DrawnUI OnHovered.
    pub fn on_hovered<S: Any>(mut self, mut f: impl FnMut(&mut crate::Mut<'_, T>, &mut S, &mut Cx<'_>, bool) + 'static) -> Self {
        if self.base.p.receives_hover.is_none() {
            self.base.p.receives_hover = Some(true);
        }
        self.input().hovered = Some(Box::new(move |raw, state, cx, on| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| crate::tree::wrong_state::<S>());
            f(&mut raw.typed(), state, cx, on)
        }));
        self
    }

    /// The control became `Ui::focused` (true) or lost it (false).
    pub fn on_focus_changed<S: Any>(mut self, mut f: impl FnMut(&mut crate::Mut<'_, T>, &mut S, &mut Cx<'_>, bool) + 'static) -> Self {
        self.input().focus_changed = Some(Box::new(move |raw, state, cx, focused| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| crate::tree::wrong_state::<S>());
            f(&mut raw.typed(), state, cx, focused)
        }));
        self
    }
}

impl Cx<'_> {
    /// The gesture being routed while a gesture handler runs (tapped, long press, context menu,
    /// `consume_gestures`): the button of a tap, canvas pixels, the velocity. `None` in other
    /// handlers.
    pub fn gesture(&self) -> Option<&Gesture> {
        self.tree.gesture.as_ref()
    }

    /// Where the gesture is, in points from the top-left of the control whose gesture handler
    /// runs (React GetOffsetInsideControlInPoints): through its transforms and its rendering scale
    /// (a RescalingLayout's fitted one). `None` outside gesture handlers.
    pub fn gesture_point(&self) -> Option<Point> {
        self.tree.gesture_point
    }

    /// The control gets every key while it is mounted, focused or not (React
    /// KeyboardManager.Subscribe: a game, a shortcut layer): its `Control::on_key` and its key
    /// handlers, after the focused control. `false` stops it.
    pub fn listen_keys(&mut self, id: impl Into<ControlId>, on: bool) {
        let id = id.into();
        self.tree.key_listeners.retain(|l| *l != id);
        if on {
            self.tree.key_listeners.push(id);
        }
    }

    /// Puts text on the system clipboard (the host does it after the frame; the browser needs
    /// the frame to follow a key or a tap, which it does).
    pub fn set_clipboard(&mut self, text: impl Into<String>) {
        self.tree.clipboard = Some(text.into());
        self.tree.needs_frame = true;
    }

    /// Opens a link in the system browser (a new tab on the web), after the frame. Only http,
    /// https and mailto links are opened.
    pub fn open_url(&mut self, url: impl Into<String>) {
        self.tree.open_urls.push(url.into());
        self.tree.needs_frame = true;
    }

    /// Asks the host for the clipboard's text (an editor on Ctrl+V): it arrives as typed text,
    /// `KeyKind::Char`, like any other. The browser's own paste over a focused editor needs no
    /// request: it comes the same way.
    pub fn request_paste(&mut self) {
        self.tree.paste_requested = true;
        self.tree.needs_frame = true;
    }

    /// The accessibility snapshot as last built (empty when no host renders it).
    pub fn accessibility_nodes(&self) -> &[AccessibilityNode] {
        &self.tree.accessibility
    }

    /// The control the accessibility overlay's keyboard focus is on.
    pub fn accessibility_focused(&self) -> Option<ControlId> {
        self.tree.accessibility_focused.filter(|f| self.tree.node(*f).is_some())
    }

    /// The canvas in points, as of the frame being drawn.
    pub fn canvas_size(&self) -> Size {
        self.tree.canvas_size
    }

    /// The safe area the platform reported, points (DrawnUI `Super.Screen` Top/Bottom/Left/
    /// RightInset; React Super.Insets: the browser's `env(safe-area-inset-*)`); zero on the
    /// desktop. Reported in both `Ui::mobile_fullscreen` modes.
    pub fn safe_insets(&self) -> crate::types::Thickness {
        self.tree.safe_insets
    }

    /// `Ui::mobile_fullscreen`: the root covers the system bars and content keeps out of them itself.
    pub fn mobile_fullscreen(&self) -> bool {
        self.tree.mobile_fullscreen
    }

    /// The part of the safe area that content must keep out of itself: `safe_insets` when
    /// fullscreen, zero otherwise (the root is already inside it).
    pub fn content_insets(&self) -> crate::types::Thickness {
        if self.tree.mobile_fullscreen { self.tree.safe_insets } else { crate::types::Thickness::default() }
    }

    /// The browser's URL hash ("#/page"), as the page opened and after every move in its
    /// history; empty on the desktop.
    pub fn location_hash(&self) -> &str {
        &self.tree.location_hash
    }

    /// The host keeps a browser history (the web): `history` changes it and back / forward come
    /// back through `Control::on_history`. False on the desktop, where a back that waits for the
    /// browser would wait forever.
    pub fn has_history(&self) -> bool {
        self.tree.history_enabled
    }

    /// Changes the browser's history after the frame (React SkiaShell UseBrowserHistory).
    pub fn history(&mut self, op: HistoryOp) {
        self.tree.history_ops.push(op);
        self.tree.needs_frame = true;
    }

    /// The control gets `Control::on_history` when the browser moves in its history.
    pub fn listen_history(&mut self, id: impl Into<ControlId>, on: bool) {
        let id = id.into();
        self.tree.history_listeners.retain(|l| *l != id);
        if on {
            self.tree.history_listeners.push(id);
        }
    }

    /// Timing of the drawn frames (React Canvas.FrameTime / FPS).
    pub fn frame_stats(&self) -> FrameStats {
        self.tree.frame_stats
    }

    /// Moves the keyboard focus to a control (`None` clears it), applied right after the current
    /// handler or gesture.
    pub fn focus(&mut self, id: Option<impl Into<ControlId>>) {
        self.tree.focus_request = Some(id.map(Into::into));
    }

    /// The control keys go to (DrawnUI FocusedChild).
    pub fn focused(&self) -> Option<ControlId> {
        self.tree.focused.filter(|f| self.tree.node(*f).is_some())
    }
}

impl<S: 'static> App for Ui<S> {
    fn rendering_mode(&self) -> RenderingModeType {
        self.rendering_mode
    }

    fn gpu_backend(&self) -> GpuBackend {
        self.gpu_backend
    }

    fn background(&self) -> crate::skia::Color {
        self.background
    }

    fn gestures(&self) -> GesturesMode {
        self.gestures
    }

    fn prepare(&mut self, width: f32, height: f32, scale: f32, time_ms: f64) {
        Ui::prepare(self, width, height, scale, time_ms)
    }

    fn page_font(&mut self, alias: &str, url: &str, weight: i32, host: &mut Host) {
        let index = self.font_urls.len();
        self.font_urls.push((alias.to_owned(), url.to_owned(), weight, true, true));
        self.fonts_pending += 1;
        host.fetch(index as u32, url);
    }

    fn init(&mut self, host: &mut Host) {
        for (index, (_, url, ..)) in self.font_urls.iter().enumerate() {
            host.fetch(index as u32, url);
        }
        // Images preloaded before the first frame.
        for request in self.tree.images.take_requests() {
            host.fetch_image(request);
        }
        for (id, url) in self.tree.assets.take_requests() {
            host.fetch(id, &url);
        }
    }

    fn baked(&mut self, id: u32, image: Option<crate::skia::Image>) {
        crate::paint::bake_arrived(&mut self.tree, id, image);
    }

    fn image(&mut self, id: u32, decoded: Option<Decoded>) {
        images::deliver(&mut self.tree, id, decoded);
    }

    fn asset(&mut self, id: u32, bytes: Vec<u8>) {
        if id >= crate::assets::ASSET_BASE {
            if let Some(deliver) = self.tree.assets.take(id) {
                deliver(&mut self.tree, bytes);
                self.tree.needs_frame = true;
            }
            return;
        }
        let Some((alias, url, weight, gates, page)) = self.font_urls.get(id as usize) else { return };
        if *gates {
            self.fonts_pending = self.fonts_pending.saturating_sub(1);
        }
        self.tree.needs_frame = true;
        if bytes.is_empty() {
            eprintln!("drawnui: font {url} did not load");
        } else if self.fonts.add_face(alias, *weight, &bytes, !page) {
            // Every text may change its size.
            let ids: Vec<ControlId> = self.tree.nodes.iter().flatten().map(|n| n.id).collect();
            for id in ids {
                self.tree.invalidate(id, Dirty::MEASURE);
            }
        }
    }

    fn pointer(&mut self, kind: PointerKind, x: f32, y: f32, time_ms: f64) {
        Ui::pointer(self, kind, x, y, time_ms)
    }

    fn pointer_button(&mut self, kind: PointerKind, button: MouseButton, x: f32, y: f32, time_ms: f64) {
        Ui::pointer_button(self, kind, button, x, y, time_ms)
    }

    fn pointer_touch(&mut self, kind: PointerKind, x: f32, y: f32, time_ms: f64) {
        Ui::pointer_touch(self, kind, x, y, time_ms)
    }

    fn wheel(&mut self, x: f32, y: f32, delta: f32, horizontal: bool, time_ms: f64) -> bool {
        Ui::wheel_on_axis(self, x, y, delta, horizontal, time_ms)
    }

    fn context_menu(&mut self, x: f32, y: f32, source: ContextMenuSource, time_ms: f64) -> bool {
        Ui::context_menu(self, x, y, source, time_ms)
    }

    fn key(&mut self, kind: KeyKind, key: InputKey, text: &str, modifiers: Modifiers, repeat: bool) -> bool {
        Ui::key(self, kind, key, text, modifiers, repeat)
    }

    fn focus_out(&mut self) {
        Ui::focus_out(self)
    }

    fn back(&mut self) -> bool {
        Ui::system_back(self)
    }

    fn blur(&mut self) {
        Ui::blur(self)
    }

    fn accessibility_activate(&mut self, id: u32) {
        Ui::accessibility_activate(self, id)
    }

    fn accessibility_focused_id(&self) -> Option<u32> {
        Ui::accessibility_focused(self).map(|f| f.index)
    }

    fn accessibility_focus(&mut self, id: u32, focused: bool) {
        Ui::accessibility_focus(self, id, focused)
    }

    fn accessibility_adjust(&mut self, id: u32, up: bool) {
        Ui::accessibility_adjust(self, id, up)
    }

    fn accessibility_scroll_into_view(&mut self, id: u32) {
        Ui::accessibility_scroll_into_view(self, id)
    }

    fn accessibility_set_value(&mut self, id: u32, value: f64) {
        Ui::accessibility_set_value(self, id, value)
    }

    fn accessibility_scroll(&mut self, id: u32, x: f32, y: f32) {
        Ui::accessibility_scroll(self, id, x, y)
    }

    fn visibility(&mut self, visible: bool) {
        if let Some(handler) = self.visibility.as_mut() {
            handler(&mut self.state, visible, &mut Cx { tree: &mut self.tree });
            self.state_dirty = true;
            self.tree.needs_frame = true;
            self.apply_focus_request();
        }
    }

    fn safe_insets(&mut self, insets: crate::types::Thickness) {
        if insets != self.tree.safe_insets {
            self.tree.safe_insets = insets;
            self.insets_changed = true;
            self.tree.needs_frame = true;
        }
    }

    fn location(&mut self, hash: &str, popped: Option<u32>) {
        hash.clone_into(&mut self.tree.location_hash);
        let Some(depth) = popped else { return };
        let tree = &mut self.tree;
        tree.history_listeners.retain(|id| tree.nodes.get(id.index as usize).is_some_and(|n| n.as_ref().is_some_and(|n| n.id == *id)));
        for i in 0..tree.history_listeners.len() {
            let Some(&id) = tree.history_listeners.get(i) else { break };
            let hash = std::mem::take(&mut tree.location_hash);
            with_kind(tree, id, |kind, cx| kind.on_history(cx, depth, &hash));
            tree.location_hash = hash;
        }
        self.state_dirty = true;
        self.tree.needs_frame = true;
        self.apply_focus_request();
    }

    fn frame(&mut self, frame: &mut Frame) -> bool {
        self.tree.history_enabled = frame.host.history_on;
        self.keyboard_navigation = frame.host.keyboard_navigation;
        self.tree.bakes.enabled = frame.host.bake_workers;
        if frame.host.accessibility_on != self.accessibility_on {
            self.set_accessibility_enabled(frame.host.accessibility_on);
        }
        self.draw(frame.surface.canvas(), frame.gpu, frame.width, frame.height, frame.scale, frame.time_ms);
        // Image loads asked for during the frame go to the host.
        for request in self.tree.images.take_requests() {
            frame.host.fetch_image(request);
        }
        for (id, url) in self.tree.assets.take_requests() {
            frame.host.fetch(id, &url);
        }
        frame.host.bakes.append(&mut self.tree.bakes.requests);
        if std::mem::take(&mut self.cursor_changed) {
            frame.host.set_cursor(self.cursor);
        }
        if std::mem::take(&mut self.text_input_changed) {
            frame.host.set_text_input(self.text_input);
            frame.host.text_input_node = self.focused().map(|f| f.index);
        }
        if let Some(id) = self.keyboard_moved.take() {
            frame.host.keyboard_moved = Some(id);
        }
        if std::mem::take(&mut self.accessibility_changed) {
            frame.host.set_accessibility(self.tree.accessibility.clone());
        }
        if let Some(text) = self.tree.clipboard.take() {
            frame.host.set_clipboard(text);
        }
        for url in self.tree.open_urls.drain(..) {
            frame.host.open_url(&url);
        }
        frame.host.history.append(&mut self.tree.history_ops);
        if std::mem::take(&mut self.tree.paste_requested) {
            frame.host.request_paste();
        }
        frame.host.wake_ms = self.wake_at();
        self.needs_frame()
    }
}

#[cfg(test)]
mod page_font_tests {
    use super::*;

    const OPEN_SANS: &[u8] = include_bytes!("../../examples/bench/assets/OpenSans-Regular.ttf");
    const INTER: &[u8] = include_bytes!("../tests/fonts/Inter-Regular.ttf");

    /// The page's font is fetched at once, the first frame waits for it, and it never becomes the
    /// default font: only labels that name it use it.
    #[test]
    fn a_page_font_is_waited_for_and_never_the_default() {
        for own in [true, false] {
            let ui = Ui::new((), |_| crate::controls::label::SkiaLabel::new("Hello"));
            let mut ui = if own { ui.font_bytes("Default", OPEN_SANS) } else { ui };
            let mut host = Host::default();
            ui.init(&mut host);
            App::page_font(&mut ui, "MyFont", "fonts/my.ttf", 400, &mut host);
            assert_eq!(host.requests.last(), Some(&(0, "fonts/my.ttf".to_owned())));
            assert!(!ui.needs_frame(), "nothing is laid out before the page's font is there");

            App::asset(&mut ui, 0, INTER.to_vec());
            assert!(ui.needs_frame());
            let family = ui.fonts.font("MyFont", 12.0).map(|f| f.typeface().family_name());
            assert_eq!(family.as_deref(), Some("Inter"));
            assert_eq!(ui.fonts.default_alias(), own.then_some("Default"), "own fonts: {own}");
        }
    }
}
