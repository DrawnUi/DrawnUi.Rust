//! The control tree: an arena of nodes addressed by generational ids. Parents and children hold
//! ids, never references. Builders make detached subtrees; `Tree::mount` moves them in.

use std::any::{Any, type_name};
use std::cell::RefCell;
use std::marker::PhantomData;
use std::ops::Deref;

use skia_safe::{Color, Matrix, Point, Rect, Size};

use crate::animators::Animator;
use crate::control::{Control, part, part_mut};
use crate::paint::{CachedObject, PaintCache};
use crate::types::{CacheType, Dirty, IntoProp, LayoutOptions, LockTouch, SkiaGradient, Thickness};

// ---------------------------------------------------------------- ids

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ControlId {
    pub(crate) index: u32,
    pub(crate) generation: u32,
}

thread_local! {
    // (generation per index, free indices). Builders have no tree to ask, so ids come from here.
    // ponytail: one allocator per thread shared by every Tree, so a second tree gets sparse slots,
    // and a Build dropped unmounted leaks its index. Move it into the tree when either matters.
    static IDS: RefCell<(Vec<u32>, Vec<u32>)> = const { RefCell::new((Vec::new(), Vec::new())) };
}

fn reserve_id() -> ControlId {
    IDS.with_borrow_mut(|(generations, free)| {
        let index = free.pop().unwrap_or_else(|| {
            generations.push(0);
            generations.len() as u32 - 1
        });
        ControlId { index, generation: generations[index as usize] }
    })
}

fn release_id(id: ControlId) {
    IDS.with_borrow_mut(|(generations, free)| {
        generations[id.index as usize] += 1;
        free.push(id.index);
    })
}

/// Typed, copyable reference to a control. `Default` points at nothing.
pub struct Handle<T>(ControlId, PhantomData<fn() -> T>);

impl<T> Handle<T> {
    pub fn id(self) -> ControlId {
        self.0
    }
}
impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Handle<T> {}
impl<T> Default for Handle<T> {
    fn default() -> Self {
        Handle(ControlId { index: u32::MAX, generation: 0 }, PhantomData)
    }
}
impl<T> From<Handle<T>> for ControlId {
    fn from(h: Handle<T>) -> Self {
        h.0
    }
}

// ---------------------------------------------------------------- node

/// State every control has: the common properties and the layout results.
#[derive(Debug)]
pub struct Base {
    pub p: ControlProps,
    /// Measured size in pixels, margin included.
    pub measured: Size,
    /// Where the control draws, in pixels (DrawingRect).
    pub rect: Rect,
    /// Pixels per point used by the last measure (RenderingScale).
    pub scale: f32,
    pub(crate) need_measure: bool,
    pub(crate) need_arrange: bool,
    pub(crate) last_constraints: (f32, f32),
    pub(crate) last_destination: Rect,
    /// Grows whenever the pixels of this subtree may have changed; caches compare against it.
    pub(crate) content_epoch: u32,
    /// Grows when the control itself changed (DRAW, MEASURE), not what is below it: a backdrop
    /// over it tells from it that its own paint changed.
    pub(crate) own_epoch: u32,
    pub(crate) dirty: Dirty,
    /// Pixels the children are moved by at paint time (a scroll's viewport offset). Layout does
    /// not see it: children keep their rects, only drawing and hit testing shift.
    pub content_offset: Point,
    /// The control arranges by what is visible (a virtualized list): it is arranged again
    /// whenever an ancestor's `content_offset` changes.
    pub(crate) tracks_viewport: bool,
    /// The item a recycled cell shows (DrawnUI ContextIndex); `None` for every other control.
    pub context_index: Option<usize>,
    /// Pixels the content above the visible rows of a virtualized list grew by during its last
    /// arrange. A scroll takes it right after arranging its content (`LayoutCx::take_viewport_shift`)
    /// and moves its offset back by it, so the rows on screen stay where they are.
    pub viewport_shift: f32,
    /// Effects attached to the control (DrawnUI VisualEffects): `Build::visual_effect`,
    /// `Mut::add_visual_effect`. Empty for most controls; an empty list allocates nothing.
    pub visual_effects: Vec<Box<dyn crate::effects::SkiaEffect>>,
    /// The mouse is over the control and it takes hover (`is_hovered`).
    pub(crate) hovered: bool,
}

impl Base {
    /// The mouse is over the control and it takes hover (DrawnUI IsHovered): true on every
    /// control under the pointer that takes hover, a card and the button inside it alike.
    pub fn is_hovered(&self) -> bool {
        self.hovered
    }
}

impl Default for Base {
    fn default() -> Self {
        Self {
            p: ControlProps::default(),
            measured: Size::default(),
            rect: Rect::default(),
            scale: 1.0,
            need_measure: true,
            need_arrange: true,
            last_constraints: (f32::NAN, f32::NAN),
            last_destination: Rect::default(),
            content_epoch: 0,
            own_epoch: 0,
            dirty: Dirty::NONE,
            content_offset: Point::default(),
            tracks_viewport: false,
            context_index: None,
            viewport_shift: 0.0,
            visual_effects: Vec::new(),
            hovered: false,
        }
    }
}

pub(crate) type Observer = Box<dyn FnMut(Raw<'_>, &dyn Any)>;
pub(crate) type Tapped = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>)>;

#[derive(Default)]
pub(crate) struct Handlers {
    pub observers: Vec<Observer>,
    pub tapped: Option<Tapped>,
    /// The rarer handlers (long press, context menu, keys, focus), boxed together so a node
    /// without them stays small.
    pub input: Option<Box<crate::ui::InputHandlers>>,
}

pub struct Node {
    pub(crate) id: ControlId,
    pub(crate) parent: Option<ControlId>,
    pub(crate) children: Vec<ControlId>,
    /// A part its parent made itself (a toggle's look), not a child the app put there: the
    /// children operations (insert, move, replace, clear) leave it in its place (C# a subview
    /// that is not an `IsChildrenItem`).
    pub(crate) part: bool,
    pub base: Base,
    /// `None` only while one of its own hooks runs.
    pub(crate) kind: Option<Box<dyn Control>>,
    pub(crate) handlers: Handlers,
}

/// What painting keeps per node. The only state a paint pass writes.
#[derive(Default)]
pub(crate) struct RenderSlot {
    pub cache: Option<CachedObject>,
    pub cache_epoch: u32,
    /// Transform applied at the last paint, canvas space. Gestures map through its inverse.
    pub matrix: Option<Matrix>,
    /// Times the cache was recorded. Diagnostics: tests prove with it that a change did not re-record.
    pub records: u32,
    /// Effects margin of the control and its descendants, pixels, with the `content_epoch` and
    /// scale it was computed for (DrawnUI AggregatedEffectsMarginPixels).
    pub effects: Option<(u32, f32, Thickness)>,
    /// Shaders, paths and filters the control's paint reuses between frames.
    pub paints: Option<Box<PaintCache>>,
    /// Its picture holds a copy of what is under it (a SkiaBackdrop inside an Operations cache),
    /// made when the canvas showed `below` (`paint::below_key`); it records again when that changes.
    pub reads_below: bool,
    pub below: Option<u64>,
}

// ---------------------------------------------------------------- write access

/// Untyped write access to one control, handed to erased handlers.
pub(crate) struct Raw<'a> {
    pub id: ControlId,
    pub control: &'a mut dyn Control,
    pub base: &'a mut Base,
    pub queue: &'a mut Vec<ControlId>,
}

impl<'a> Raw<'a> {
    pub(crate) fn typed<T: Control>(self) -> Mut<'a, T> {
        let control = part_mut::<T>(self.control).expect("handler bound to its own control type");
        Mut { id: self.id, control, base: self.base, queue: self.queue }
    }

    /// Write access to the control whatever its type (a handler of an effect).
    pub(crate) fn any(self) -> Mut<'a, dyn Control> {
        Mut { id: self.id, control: self.control, base: self.base, queue: self.queue }
    }
}

/// Write access to a mounted control. Setters compare, and mark the control dirty on a change.
/// Dereferences to the control for reading. `Mut<dyn Control>` reaches the common properties of
/// a control of any type.
pub struct Mut<'a, T: ?Sized> {
    id: ControlId,
    control: &'a mut T,
    base: &'a mut Base,
    queue: &'a mut Vec<ControlId>,
}

impl<T: ?Sized> Mut<'_, T> {
    pub fn id(&self) -> ControlId {
        self.id
    }
    pub fn base(&self) -> &Base {
        self.base
    }
    /// Write access to the common state. Call `mark` after changing anything.
    pub(crate) fn base_mut(&mut self) -> &mut Base {
        self.base
    }
    /// For property setters (the `props!` macro). Call `mark` after changing anything.
    pub fn control_mut(&mut self) -> &mut T {
        self.control
    }
    pub fn mark(&mut self, dirty: Dirty) {
        if dirty.is_empty() {
            return;
        }
        if self.base.dirty.is_empty() {
            self.queue.push(self.id);
        }
        self.base.dirty |= dirty;
    }
}

impl<T: ?Sized> Deref for Mut<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.control
    }
}

// ---------------------------------------------------------------- common properties

/// Declares the properties every control has: the `ControlProps` struct, a builder method on
/// every `Build<T>` and a setter on every `Mut<T>`.
macro_rules! base_props {
    ($($(#[$doc:meta])* $name:ident / $setter:ident : $ty:ty = $default:expr, $dirty:ident;)*) => {
        #[derive(Clone, Debug, PartialEq)]
        pub struct ControlProps { $($(#[$doc])* pub $name: $ty,)* }

        impl Default for ControlProps {
            fn default() -> Self { Self { $($name: $default,)* } }
        }

        impl<T: Control> Build<T> {
            $($(#[$doc])* pub fn $name(mut self, v: impl IntoProp<$ty>) -> Self {
                self.base.p.$name = v.into_prop();
                self
            })*
        }

        impl<T: ?Sized> Mut<'_, T> {
            $(pub fn $setter(&mut self, v: impl IntoProp<$ty>) {
                let v = v.into_prop();
                if self.base.p.$name != v {
                    self.base.p.$name = v;
                    self.mark(Dirty::$dirty);
                }
            })*
        }
    };
}

base_props! {
    horizontal_options / set_horizontal_options: LayoutOptions = LayoutOptions::Start, MEASURE;
    vertical_options / set_vertical_options: LayoutOptions = LayoutOptions::Start, MEASURE;
    /// Points; -1 = auto.
    width_request / set_width_request: f32 = -1.0, MEASURE;
    height_request / set_height_request: f32 = -1.0, MEASURE;
    minimum_width_request / set_minimum_width_request: f32 = -1.0, MEASURE;
    minimum_height_request / set_minimum_height_request: f32 = -1.0, MEASURE;
    maximum_width_request / set_maximum_width_request: f32 = -1.0, MEASURE;
    maximum_height_request / set_maximum_height_request: f32 = -1.0, MEASURE;
    margin / set_margin: Thickness = Thickness::ZERO, MEASURE;
    padding / set_padding: Thickness = Thickness::ZERO, MEASURE;
    /// 0 = off; positive = square of the larger side, negative = square of the smaller one.
    lock_ratio / set_lock_ratio: f32 = 0.0, MEASURE;
    horizontal_fill_ratio / set_horizontal_fill_ratio: f32 = 1.0, MEASURE;
    vertical_fill_ratio / set_vertical_fill_ratio: f32 = 1.0, MEASURE;
    /// The cell in a Grid parent (DrawnUI SkiaLayout.Column / Row / ColumnSpan / RowSpan).
    column / set_column: i32 = 0, MEASURE;
    row / set_row: i32 = 0, MEASURE;
    column_span / set_column_span: i32 = 1, MEASURE;
    row_span / set_row_span: i32 = 1, MEASURE;
    is_visible / set_is_visible: bool = true, MEASURE;
    /// Points beyond the visible area, on each side, that still count as visible for a control
    /// that realizes only what is visible.
    virtualisation_inflated / set_virtualisation_inflated: f32 = 0.0, MEASURE;
    background_color / set_background_color: Option<Color> = None, DRAW;
    /// Painted instead of the plain background color (DrawnUI FillGradient).
    fill_gradient / set_fill_gradient: Option<Box<SkiaGradient>> = None, DRAW;
    use_cache / set_use_cache: CacheType = CacheType::None, DRAW;
    is_clipped_to_bounds / set_is_clipped_to_bounds: bool = false, REPAINT;
    /// True: `is_clipped_to_bounds` cuts at the rect, and overlay effects (the ripple) are clipped
    /// to the control's shape. False: that clip is grown by what the subtree paints outside (a
    /// child's shadow stays) and overlays are not clipped (DrawnUI ClipEffects, as DrawnUi.React).
    clip_effects / set_clip_effects: bool = true, REPAINT;
    translation_x / set_translation_x: f32 = 0.0, REPAINT;
    translation_y / set_translation_y: f32 = 0.0, REPAINT;
    /// Degrees, around the anchor.
    rotation / set_rotation: f32 = 0.0, REPAINT;
    scale_x / set_scale_x: f32 = 1.0, REPAINT;
    scale_y / set_scale_y: f32 = 1.0, REPAINT;
    skew_x / set_skew_x: f32 = 0.0, REPAINT;
    skew_y / set_skew_y: f32 = 0.0, REPAINT;
    anchor_x / set_anchor_x: f32 = 0.5, REPAINT;
    anchor_y / set_anchor_y: f32 = 0.5, REPAINT;
    opacity / set_opacity: f32 = 1.0, REPAINT;
    /// Paint-time offset in points; a cached control is blitted at the offset with no matrix.
    left / set_left: f32 = 0.0, REPAINT;
    top / set_top: f32 = 0.0, REPAINT;
    /// Higher draws later and receives gestures first.
    z_index / set_z_index: i32 = 0, REPAINT;
    input_transparent / set_input_transparent: bool = false, NONE;
    block_gestures_below / set_block_gestures_below: bool = false, NONE;
    /// The control takes hover: `is_hovered` and `on_hovered` follow the mouse over it (DrawnUI
    /// ReceivesHover). `None` = the control's default (`Control::receives_hover`: buttons, sliders,
    /// toggles, radio buttons, carousels, drawers); a tapped handler does not make a control hover.
    receives_hover / set_receives_hover: Option<bool> = None, NONE;
    lock_children_gestures / set_lock_children_gestures: LockTouch = LockTouch::Disabled, NONE;
    /// Color of touch feedback effects (the ripple).
    touch_effect_color / set_touch_effect_color: Color = Color::WHITE, NONE;
    /// Feedback played where a tap lands when the control's tapped handler runs (DrawnUI
    /// AnimationTapped): `Ripple` in `touch_effect_color`.
    animation_tapped / set_animation_tapped: crate::controls::button::SkiaTouchAnimation = crate::controls::button::SkiaTouchAnimation::None, NONE;
    /// Milliseconds of the tap feedback; 0 = the animator's default (500 for the ripple).
    animation_tapped_speed / set_animation_tapped_speed: f32 = 0.0, NONE;
    tag / set_tag: String = String::new(), NONE;
    /// A tap makes the control `Ui::focused`; false: a tap on it leaves the focus where it is
    /// (DrawnUI CanBeFocused). Controls that take focus themselves call `GestureCx::focus`.
    can_be_focused / set_can_be_focused: bool = false, NONE;
    /// While focused, a tap on another control or on nothing does not take the focus away.
    lock_focus / set_lock_focus: bool = false, NONE;
    /// ARIA role (`Aria::BUTTON`, ...); set, the control is in the accessibility tree.
    /// `Aria::PRESENTATION` keeps it out even when a default applies.
    accessibility_role / set_accessibility_role: &'static str = "", NONE;
    /// Spoken label; empty = the control's own text (`Control::accessibility_label`).
    accessibility_label / set_accessibility_label: String = String::new(), NONE;
    accessibility_hint / set_accessibility_hint: String = String::new(), NONE;
    /// Tab stop and activation in the overlay; unset = true when the control has a tapped handler.
    accessibility_can_interact / set_accessibility_can_interact: Option<bool> = None, NONE;
    /// aria-pressed / aria-checked for toggles; `None` = not a toggle.
    accessibility_is_pressed / set_accessibility_is_pressed: Option<bool> = None, NONE;
    /// The overlay carries the control's text as real, invisible text in its lines (React
    /// AccessibilityTextSelectable, off by default): the browser selects and copies it, a screen
    /// reader reads it word by word. Pointer input over that text goes to the selection, not to
    /// the control: never on gesture-driven controls.
    accessibility_text_selectable / set_accessibility_text_selectable: bool = false, NONE;
    /// aria-live: `Aria::LIVE_POLITE` or `Aria::LIVE_ASSERTIVE`; changes are announced.
    accessibility_live / set_accessibility_live: &'static str = "", NONE;
}

impl<T: Control> Build<T> {
    /// Fills the parent on both axes.
    pub fn fill(self) -> Self {
        self.horizontal_options(LayoutOptions::Fill).vertical_options(LayoutOptions::Fill)
    }
    pub fn fill_x(self) -> Self {
        self.horizontal_options(LayoutOptions::Fill)
    }
    pub fn fill_y(self) -> Self {
        self.vertical_options(LayoutOptions::Fill)
    }
    pub fn center(self) -> Self {
        self.horizontal_options(LayoutOptions::Center).vertical_options(LayoutOptions::Center)
    }
    /// Sets scale_x and scale_y together.
    pub fn scale(self, v: impl IntoProp<f32>) -> Self {
        let v = v.into_prop();
        self.scale_x(v).scale_y(v)
    }
}

// ---------------------------------------------------------------- builder

/// A control under construction. Its id is reserved already, so `assign` hands out a real handle.
pub struct Build<T: Control> {
    id: ControlId,
    pub(crate) base: Base,
    control: T,
    children: Vec<Detached>,
    pub(crate) handlers: Handlers,
}

/// A type-erased subtree, ready to mount.
pub struct Detached {
    id: ControlId,
    base: Base,
    control: Box<dyn Control>,
    children: Vec<Detached>,
    handlers: Handlers,
}

impl<T: Control> From<Build<T>> for Detached {
    fn from(b: Build<T>) -> Self {
        Detached { id: b.id, base: b.base, control: Box::new(b.control), children: b.children, handlers: b.handlers }
    }
}

// The engine is not generic over the app state: handlers get it as `dyn Any` and downcast here.
pub(crate) fn wrong_state<S>() -> ! {
    panic!("handler expects app state `{}`", type_name::<S>())
}

impl<T: Control> Build<T> {
    pub fn new(control: T) -> Self {
        Build { id: reserve_id(), base: Base::default(), control, children: Vec::new(), handlers: Handlers::default() }
    }

    pub fn id(&self) -> ControlId {
        self.id
    }

    /// For property builders (the `props!` macro).
    pub fn control_mut(&mut self) -> &mut T {
        &mut self.control
    }

    /// Stores a typed handle to this control.
    pub fn assign(self, slot: &mut Handle<T>) -> Self {
        *slot = Handle(self.id, PhantomData);
        self
    }

    pub(crate) fn handle(&self) -> Handle<T> {
        Handle(self.id, PhantomData)
    }

    /// Runs once at mount and again whenever the app state may have changed.
    pub fn observe<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, T>, &S) + 'static) -> Self {
        self.handlers.observers.push(Box::new(move |raw, state| {
            let state = state.downcast_ref::<S>().unwrap_or_else(|| wrong_state::<S>());
            f(&mut raw.typed(), state)
        }));
        self
    }

    pub fn on_tapped<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, T>, &mut S, &mut Cx<'_>) + 'static) -> Self {
        self.handlers.tapped = Some(Box::new(move |raw, state, cx| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
            f(&mut raw.typed(), state, cx)
        }));
        self
    }

    pub(crate) fn push_child(&mut self, child: impl Into<Detached>) {
        self.children.push(child.into());
    }
}

/// Controls that host children.
pub trait Container: Control {}

impl<T: Container> Build<T> {
    pub fn children(mut self, children: impl IntoChildren) -> Self {
        children.push_into(&mut self.children);
        self
    }
}

pub trait IntoChildren {
    fn push_into(self, out: &mut Vec<Detached>);
}

impl IntoChildren for Detached {
    fn push_into(self, out: &mut Vec<Detached>) {
        out.push(self)
    }
}
impl<T: Control> IntoChildren for Build<T> {
    fn push_into(self, out: &mut Vec<Detached>) {
        out.push(self.into())
    }
}
impl<C: IntoChildren> IntoChildren for Option<C> {
    fn push_into(self, out: &mut Vec<Detached>) {
        if let Some(c) = self {
            c.push_into(out)
        }
    }
}
impl<C: IntoChildren> IntoChildren for Vec<C> {
    fn push_into(self, out: &mut Vec<Detached>) {
        for c in self {
            c.push_into(out)
        }
    }
}

macro_rules! tuple_children {
    () => {};
    ($head:ident $($tail:ident)*) => {
        impl<$head: IntoChildren, $($tail: IntoChildren),*> IntoChildren for ($head, $($tail,)*) {
            #[allow(non_snake_case)]
            fn push_into(self, out: &mut Vec<Detached>) {
                let ($head, $($tail,)*) = self;
                $head.push_into(out);
                $($tail.push_into(out);)*
            }
        }
        tuple_children!($($tail)*);
    };
}
tuple_children!(A B C D E F G H I J K L);

// ---------------------------------------------------------------- tree

#[derive(Default)]
pub struct Tree {
    pub(crate) nodes: Vec<Option<Node>>,
    pub(crate) render: Vec<RenderSlot>,
    pub(crate) root: Option<ControlId>,
    /// Controls with pending dirty flags, drained by the commit step of a frame.
    pub(crate) queue: Vec<ControlId>,
    /// The emptied list of the last commit, kept for its allocation.
    pub(crate) queue_spare: Vec<ControlId>,
    pub(crate) removals: Vec<ControlId>,
    /// Caches replaced during a frame; dropped after the frame was submitted.
    pub(crate) drops: Vec<CachedObject>,
    pub(crate) needs_frame: bool,
    /// Running animators, oldest first. Ticked once per frame; paint reads their overlays.
    pub(crate) animators: Vec<Animator>,
    /// Id of the newest animator ever started.
    pub(crate) last_animation: u32,
    /// Frame time of the frame being drawn (the last one, between frames).
    pub(crate) time_ms: f64,
    /// The frame clock is synthetic (the headless host): work budgets count work, not time, so a
    /// test gives the same frames on every machine.
    pub(crate) synthetic_clock: bool,
    /// Controls that arrange by what is visible (`LayoutCx::track_viewport`).
    pub(crate) trackers: Vec<ControlId>,
    /// The image manager: loads, the decoded bitmaps by source, `preload`.
    pub images: crate::images::Images,
    /// Asset loads of controls (SVG, Lottie, shader sources): urls out, bytes back.
    pub(crate) assets: crate::assets::Assets,
    /// ImageDoubleBuffered caches out with the host's workers.
    pub(crate) bakes: crate::bakes::Bakes,
    /// The `Gpu::epoch` the caches and kept surfaces were made on.
    pub(crate) gpu_epoch: u32,
    /// The control keys go to (DrawnUI FocusedChild).
    pub(crate) focused: Option<ControlId>,
    /// A control asked for the focus (`Some(id)`) or gave it up (`None`) while a gesture or key
    /// was routed; applied right after.
    pub(crate) focus_request: Option<Option<ControlId>>,
    /// The gesture being routed, for handlers (`Cx::gesture`), and where it is, points from the
    /// top-left of the control whose handler runs (`Cx::gesture_point`).
    pub(crate) gesture: Option<crate::gestures::Gesture>,
    pub(crate) gesture_point: Option<skia_safe::Point>,
    /// Controls that get every key while mounted, focused or not (`Cx::listen_keys`).
    pub(crate) key_listeners: Vec<ControlId>,
    /// The last accessibility snapshot and the control the overlay's focus is on.
    pub(crate) accessibility: Vec<crate::ui::AccessibilityNode>,
    pub(crate) accessibility_focused: Option<ControlId>,
    /// Text for the system clipboard, handed to the host after the frame (`Cx::set_clipboard`).
    pub(crate) clipboard: Option<String>,
    /// The clipboard's text is wanted as typed text for the focused control (`Cx::request_paste`).
    pub(crate) paste_requested: bool,
    /// The canvas in points, as of the frame being drawn (`Cx::canvas_size`).
    pub(crate) canvas_size: skia_safe::Size,
    /// Timing of the drawn frames (`Cx::frame_stats`).
    pub(crate) frame_stats: crate::ui::FrameStats,
    /// Links to open in the system browser, handed to the host after the frame (`Cx::open_url`).
    pub(crate) open_urls: Vec<String>,
    /// The safe area of the screen, points (`Cx::safe_insets`); zero on the desktop.
    pub(crate) safe_insets: crate::types::Thickness,
    /// `Ui::mobile_fullscreen`.
    pub(crate) mobile_fullscreen: bool,
    /// The browser's URL hash and the history entries asked for (`Cx::history`).
    pub(crate) location_hash: String,
    pub(crate) history_ops: Vec<crate::ui::HistoryOp>,
    /// The host has a browser history (`Cx::has_history`).
    pub(crate) history_enabled: bool,
    /// Controls told when the browser moves in its history (`Cx::listen_history`).
    pub(crate) history_listeners: Vec<ControlId>,
    /// What is under the mouse may have changed without the mouse moving (controls added,
    /// removed, rebound, hidden, scrolled by a jump): the frame ends with a hover check.
    pub(crate) hover_check: bool,
    /// Controls that move their content by themselves (`Control::moves_content`: scrolls,
    /// carousels, drawers): while one moves, hover waits.
    pub(crate) movers: Vec<ControlId>,
}

impl Tree {
    pub fn root(&self) -> Option<ControlId> {
        self.root
    }

    pub(crate) fn node(&self, id: ControlId) -> Option<&Node> {
        self.nodes.get(id.index as usize)?.as_ref().filter(|n| n.id == id)
    }

    pub(crate) fn node_mut(&mut self, id: ControlId) -> Option<&mut Node> {
        self.nodes.get_mut(id.index as usize)?.as_mut().filter(|n| n.id == id)
    }

    pub(crate) fn take(&mut self, id: ControlId) -> Option<Node> {
        self.nodes.get_mut(id.index as usize)?.take_if(|n| n.id == id)
    }

    pub(crate) fn put_back(&mut self, node: Node) {
        let index = node.id.index as usize;
        self.nodes[index] = Some(node);
    }

    /// Moves a detached subtree into the arena under `parent` (`None` makes it the root).
    pub fn mount(&mut self, parent: Option<ControlId>, detached: impl Into<Detached>) -> ControlId {
        let d = detached.into();
        let index = d.id.index as usize;
        if self.nodes.len() <= index {
            self.nodes.resize_with(index + 1, || None);
            self.render.resize_with(index + 1, RenderSlot::default);
        }
        if let Some(old) = self.render[index].cache.take() {
            self.drops.push(old);
        }
        self.render[index] = RenderSlot::default();
        let mut base = d.base;
        // Every new control gets one `on_props_changed` before its first layout.
        base.dirty = Dirty::APPLY;
        self.queue.push(d.id);
        if d.handlers.input.as_ref().is_some_and(|h| h.listen_keys) {
            self.key_listeners.push(d.id);
        }
        if d.handlers.input.as_ref().is_some_and(|h| h.listen_history) {
            self.history_listeners.push(d.id);
        }
        if d.control.moves_content().is_some() {
            self.movers.push(d.id);
        }
        self.hover_check = true;
        self.nodes[index] = Some(Node {
            id: d.id,
            parent,
            children: Vec::with_capacity(d.children.len()),
            part: false,
            base,
            kind: Some(d.control),
            handlers: d.handlers,
        });
        match parent.and_then(|p| self.node_mut(p)) {
            Some(p) => p.children.push(d.id),
            None => self.root = Some(d.id),
        }
        for child in d.children {
            self.mount(Some(d.id), child);
        }
        self.needs_frame = true;
        d.id
    }

    /// Removes the control and its subtree now; every handle into it goes stale.
    pub(crate) fn remove_now(&mut self, id: ControlId) {
        let Some(node) = self.take(id) else { return };
        release_id(id);
        if let Some(cache) = self.render[id.index as usize].cache.take() {
            self.drops.push(cache);
        }
        // Its shaders and paths go now, not when the slot gets its next control.
        self.render[id.index as usize] = RenderSlot::default();
        if node.base.tracks_viewport {
            self.trackers.retain(|t| *t != id);
        }
        if node.kind.as_deref().is_some_and(|k| k.moves_content().is_some()) {
            self.movers.retain(|m| *m != id);
        }
        self.hover_check = true;
        if let Some(p) = node.parent.and_then(|p| self.node_mut(p)) {
            p.children.retain(|c| *c != id);
        }
        if self.root == Some(id) {
            self.root = None;
        }
        for child in node.children {
            self.remove_now(child);
        }
    }

    pub fn children(&self, id: impl Into<ControlId>) -> &[ControlId] {
        self.node(id.into()).map_or(&[], |n| &n.children)
    }

    pub fn parent(&self, id: impl Into<ControlId>) -> Option<ControlId> {
        self.node(id.into())?.parent
    }

    pub fn base(&self, id: impl Into<ControlId>) -> Option<&Base> {
        Some(&self.node(id.into())?.base)
    }

    /// Typed lookup. Walks the inner chain, so `find::<SkiaLayout>(button)` works.
    pub fn find<T: Control>(&self, id: impl Into<ControlId>) -> Option<&T> {
        part(self.node(id.into())?.kind.as_deref()?)
    }

    pub fn find_mut<T: Control>(&mut self, id: impl Into<ControlId>) -> Option<Mut<'_, T>> {
        let id = id.into();
        let Tree { nodes, queue, .. } = self;
        let node = nodes.get_mut(id.index as usize)?.as_mut().filter(|n| n.id == id)?;
        Some(Mut { id, control: part_mut(node.kind.as_deref_mut()?)?, base: &mut node.base, queue })
    }

    pub fn get_mut<T: Control>(&mut self, handle: Handle<T>) -> Option<Mut<'_, T>> {
        self.find_mut(handle)
    }

    /// Write access to the common properties of a control of any type.
    pub fn any_mut(&mut self, id: impl Into<ControlId>) -> Option<Mut<'_, dyn Control>> {
        let id = id.into();
        let Tree { nodes, queue, .. } = self;
        let node = nodes.get_mut(id.index as usize)?.as_mut().filter(|n| n.id == id)?;
        Some(Mut { id, control: node.kind.as_deref_mut()?, base: &mut node.base, queue })
    }

    /// What handlers get: the way to start animations and change the tree from outside a handler.
    pub fn cx(&mut self) -> Cx<'_> {
        Cx { tree: self }
    }

    /// Adds a subtree as the last child of `parent`.
    pub fn add_child(&mut self, parent: impl Into<ControlId>, child: impl Into<Detached>) -> ControlId {
        Cx { tree: self }.add_child(parent, child)
    }

    /// Removes a control and its subtree before the next layout.
    pub fn remove(&mut self, id: impl Into<ControlId>) {
        Cx { tree: self }.remove(id)
    }

    /// Moves the children of a control at paint time (pixels). This is what scrolling is: no
    /// layout runs, except for descendants that arrange by the visible area.
    pub fn set_content_offset(&mut self, id: impl Into<ControlId>, offset: Point) {
        let id = id.into();
        let Some(node) = self.node_mut(id) else { return };
        if node.base.content_offset == offset {
            return;
        }
        node.base.content_offset = offset;
        // The content moved under a still mouse (a jump; a running scroll pauses hover till it stops).
        self.hover_check = true;
        // Own pixels changed: own cache and the ancestors' caches are stale.
        self.invalidate(id, Dirty::DRAW);
        self.rearrange_trackers(id, None);
    }

    /// What can be seen through `ancestor` changed (it scrolled, or it has another rect): the
    /// controls below it that arrange by what is visible are arranged again. Arrange must reach
    /// them, so every control on the way up is marked too, up to the root or up to `below`.
    pub(crate) fn rearrange_trackers(&mut self, ancestor: ControlId, below: Option<ControlId>) {
        for i in 0..self.trackers.len() {
            let tracker = self.trackers[i];
            if !self.is_ancestor(ancestor, tracker) {
                continue;
            }
            let mut current = Some(tracker);
            while let Some(node) = current.filter(|c| Some(*c) != below).and_then(|c| self.node_mut(c)) {
                node.base.need_arrange = true;
                current = node.parent;
            }
        }
    }

    pub(crate) fn is_ancestor(&self, ancestor: ControlId, id: ControlId) -> bool {
        let mut current = self.parent(id);
        while let Some(c) = current {
            if c == ancestor {
                return true;
            }
            current = self.parent(c);
        }
        false
    }

    /// The part of a control's rect that can be seen through its ancestors, in the control's own
    /// coordinates: every ancestor's rect and scroll offset applied. Render transforms are ignored.
    pub fn visible_rect(&self, id: impl Into<ControlId>) -> Rect {
        let id = id.into();
        let Some(node) = self.node(id) else { return Rect::default() };
        let mut visible = node.base.rect;
        let mut shift = Point::default();
        let mut current = node.parent;
        while let Some(ancestor) = current.and_then(|c| self.node(c)) {
            shift += ancestor.base.content_offset;
            // The ancestor's box, seen from the control's coordinates.
            if !visible.intersect(ancestor.base.rect.with_offset(-shift)) {
                return Rect::default();
            }
            current = ancestor.parent;
        }
        visible
    }

    /// Marks a control dirty from outside a setter.
    pub fn invalidate(&mut self, id: impl Into<ControlId>, dirty: Dirty) {
        let id = id.into();
        let Tree { nodes, queue, .. } = self;
        let Some(node) = nodes.get_mut(id.index as usize).and_then(|n| n.as_mut()).filter(|n| n.id == id) else { return };
        if dirty.is_empty() {
            return;
        }
        if node.base.dirty.is_empty() {
            queue.push(id);
        }
        node.base.dirty |= dirty;
    }

    /// Runs every observer against the app state.
    pub(crate) fn run_observers(&mut self, state: &dyn Any) {
        let Tree { nodes, queue, .. } = self;
        for node in nodes.iter_mut().flatten() {
            observe(node, queue, state);
        }
    }

    /// Runs the observers of a subtree mounted after the frame's observers ran (a list cell
    /// created during layout): an observer runs once at mount.
    pub(crate) fn run_observers_under(&mut self, id: ControlId, state: &dyn Any) {
        let Tree { nodes, queue, .. } = self;
        let mut stack = vec![id];
        while let Some(id) = stack.pop() {
            let Some(node) = nodes.get_mut(id.index as usize).and_then(|n| n.as_mut()).filter(|n| n.id == id) else {
                continue;
            };
            stack.extend_from_slice(&node.children);
            observe(node, queue, state);
        }
    }
}

fn observe(node: &mut Node, queue: &mut Vec<ControlId>, state: &dyn Any) {
    let Some(control) = node.kind.as_deref_mut() else { return };
    for observer in &mut node.handlers.observers {
        observer(Raw { id: node.id, control: &mut *control, base: &mut node.base, queue }, state);
    }
}

/// What a handler may reach besides its own control and the app state.
pub struct Cx<'a> {
    pub(crate) tree: &'a mut Tree,
}

impl Cx<'_> {
    pub fn get_mut<T: Control>(&mut self, handle: Handle<T>) -> Option<Mut<'_, T>> {
        self.tree.get_mut(handle)
    }

    pub fn find<T: Control>(&self, id: impl Into<ControlId>) -> Option<&T> {
        self.tree.find(id)
    }

    /// Write access to the common properties of a control of any type.
    pub fn any_mut(&mut self, id: impl Into<ControlId>) -> Option<Mut<'_, dyn Control>> {
        self.tree.any_mut(id)
    }

    pub fn base(&self, id: impl Into<ControlId>) -> Option<&Base> {
        self.tree.base(id)
    }

    /// Adds a subtree as the last child of `parent`.
    pub fn add_child(&mut self, parent: impl Into<ControlId>, child: impl Into<Detached>) -> ControlId {
        let parent = parent.into();
        let id = self.tree.mount(Some(parent), child);
        self.tree.invalidate(parent, Dirty::MEASURE);
        id
    }

    /// Moves the children of a control at paint time (pixels): scrolling.
    pub fn set_content_offset(&mut self, id: impl Into<ControlId>, offset: Point) {
        self.tree.set_content_offset(id, offset)
    }

    /// The part of a control that is visible through its ancestors, in its own coordinates.
    pub fn visible_rect(&self, id: impl Into<ControlId>) -> Rect {
        self.tree.visible_rect(id)
    }

    /// Removes a control and its subtree before the next layout.
    pub fn remove(&mut self, id: impl Into<ControlId>) {
        self.tree.removals.push(id.into());
        self.tree.needs_frame = true;
    }

    /// Adds a subtree as the app's child number `index` of `parent` (past the end: the last).
    /// It is drawn in that place (C# Children.Insert).
    pub fn insert_child(&mut self, parent: impl Into<ControlId>, index: usize, child: impl Into<Detached>) -> ControlId {
        let parent = parent.into();
        let id = self.add_child(parent, child);
        let mut order = self.app_children(parent);
        order.retain(|c| *c != id);
        order.insert(index.min(order.len()), id);
        self.reorder(parent, &order);
        id
    }

    /// Puts a subtree where `old` is and removes `old` (C# `Children[i] = child`). `None` when
    /// `old` has no parent.
    pub fn replace_child(&mut self, old: impl Into<ControlId>, child: impl Into<Detached>) -> Option<ControlId> {
        let old = old.into();
        let parent = self.tree.parent(old)?;
        let id = self.add_child(parent, child);
        let mut order = self.app_children(parent);
        order.retain(|c| *c != id);
        let at = order.iter().position(|c| *c == old)?;
        order[at] = id;
        // Drawn nowhere it matters until it goes before the next layout.
        order.push(old);
        self.reorder(parent, &order);
        self.remove(old);
        Some(id)
    }

    /// Moves the app's child number `from` of `parent` to number `to` (C# Move).
    pub fn move_child(&mut self, parent: impl Into<ControlId>, from: usize, to: usize) {
        let parent = parent.into();
        let mut order = self.app_children(parent);
        if from >= order.len() {
            return;
        }
        let id = order.remove(from);
        order.insert(to.min(order.len()), id);
        self.reorder(parent, &order);
    }

    /// Removes every child the app put under `parent` before the next layout; the parts the
    /// control made itself stay (C# Children.Clear).
    pub fn clear_children(&mut self, parent: impl Into<ControlId>) {
        for child in self.app_children(parent.into()) {
            self.remove(child);
        }
    }

    /// The children the app put under `parent`, in order: not the control's own parts, not the
    /// ones waiting to be removed.
    pub fn app_children(&self, parent: impl Into<ControlId>) -> Vec<ControlId> {
        let tree = &*self.tree;
        let own = |c: &ControlId| tree.node(*c).is_some_and(|n| n.part);
        tree.children(parent).iter().copied().filter(|c| !own(c) && !tree.removals.contains(c)).collect()
    }

    /// Puts the app's children in `order` into the places they hold now, so the control's own
    /// parts keep theirs (C# ChildrenCollectionSync), and lays the parent out again. Work only
    /// on a change, nothing per frame.
    fn reorder(&mut self, parent: ControlId, order: &[ControlId]) {
        let Some(node) = self.tree.node_mut(parent) else { return };
        let mut next = order.iter();
        for slot in node.children.iter_mut() {
            if order.contains(slot)
                && let Some(id) = next.next()
            {
                *slot = *id;
            }
        }
        self.tree.invalidate(parent, Dirty::MEASURE);
    }

    pub fn invalidate(&mut self, id: impl Into<ControlId>, dirty: Dirty) {
        self.tree.invalidate(id, dirty)
    }
}
