//! What a control type implements. Every hook has a default that delegates to the embedded base
//! part (`inner`), so a custom control overrides only what it needs.

use std::any::Any;

use skia_safe::{Canvas, Matrix, Path, Rect, Size};

use crate::animators::Animator;
use crate::fonts::Fonts;
use crate::gestures::Gesture;
use crate::gpu::Gpu;
use crate::keyboard::{Cursor, KeyEvent};
use crate::paint::CachedObject;
use crate::tree::{Base, ControlId, Cx, Node, RenderSlot, Tree};
use crate::types::{Dirty, Thickness};
use crate::{layout, paint};

/// What a control did with a gesture.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Handled {
    /// Not interested: children get it, then the control's own tapped handler.
    No,
    /// Consumed; nothing else sees it.
    Yes,
    /// Consumed, and the control's tapped handler runs now.
    Tapped,
    /// A descendant consumed it: the control asked its children first
    /// (`GestureCx::route_children`) and one of them took it (React: `return childConsumed`). The
    /// router treats that control as the consumer, the gesture owner included, and runs neither the
    /// children pass nor the handlers of the control that answered.
    By(ControlId),
}

pub trait Control: Any {
    /// The embedded base part (the C# base class), if any.
    fn inner(&self) -> Option<&dyn Control> {
        None
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        None
    }

    /// Measures the content. Constraints are pixels with the margin already taken off; an
    /// infinite one means "as much as you need". Returns the content size in pixels, padding
    /// included.
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        match self.inner_mut() {
            Some(inner) => inner.measure(cx, width, height),
            None => Size::default(),
        }
    }

    /// Runs after the drawing rect was set. Containers place their children here.
    fn arrange(&mut self, cx: &mut LayoutCx) {
        if let Some(inner) = self.inner_mut() {
            inner.arrange(cx)
        }
    }

    /// Fills the background. Default: `background_color` over the drawing rect.
    fn paint_background(&self, cx: &mut PaintCx) {
        match self.inner() {
            Some(inner) => inner.paint_background(cx),
            None => paint::paint_background_rect(cx),
        }
    }

    /// Draws the content into `cx.rect`. The tree is read-only here. Default: the children.
    fn paint(&self, cx: &mut PaintCx) {
        match self.inner() {
            Some(inner) => inner.paint(cx),
            None => cx.paint_children(),
        }
    }

    /// The shape overlay effects (the ripple) are clipped to, for a drawing rect in pixels
    /// (DrawnUI CreateClip). Default: the rect.
    fn create_clip(&self, rect: Rect, scale: f32) -> Path {
        match self.inner() {
            Some(inner) => inner.create_clip(rect, scale),
            None => Path::rect(rect, None),
        }
    }

    /// Pixels the control paints outside its drawing rect, per side: a shadow, glyph ink past the
    /// line box (DrawnUI ComputeEffectsMargin). Caches and the bounds clip grow by it and by the
    /// margins of the descendants; layout and hit testing never see it. Default: none.
    fn effects_margin(&self, scale: f32) -> Thickness {
        match self.inner() {
            Some(inner) => inner.effects_margin(scale),
            None => Thickness::ZERO,
        }
    }

    /// Sees every gesture that lands on the control before its children do. `PointerEnter` /
    /// `PointerExit` come straight to the control, not through its parents.
    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        match self.inner_mut() {
            Some(inner) => inner.on_gesture(cx, gesture),
            None => Handled::No,
        }
    }

    /// A key or typed text, while the control is focused (`Ui::focused`), before the app's
    /// handlers. True = used: the app handlers still see it, the host's default action is kept away.
    fn on_key(&mut self, cx: &mut GestureCx, event: &KeyEvent) -> bool {
        match self.inner_mut() {
            Some(inner) => inner.on_key(cx, event),
            None => false,
        }
    }

    /// A screen reader set the value of a range control (UI Automation RangeValue.SetValue,
    /// AT-SPI Value): see `accessibility_value`. True = taken.
    fn accessibility_set_value(&mut self, cx: &mut GestureCx, value: f64) -> bool {
        match self.inner_mut() {
            Some(inner) => inner.accessibility_set_value(cx, value),
            None => false,
        }
    }

    /// The control became `Ui::focused`, or stopped being it.
    fn on_focus_changed(&mut self, cx: &mut GestureCx, focused: bool) {
        if let Some(inner) = self.inner_mut() {
            inner.on_focus_changed(cx, focused)
        }
    }

    /// While focused, the control takes typed text (`KeyKind::Char`): the host opens its
    /// keyboard and IME over the control (an editor).
    fn wants_text_input(&self) -> bool {
        self.inner().is_some_and(|inner| inner.wants_text_input())
    }

    /// The mouse cursor over `local` (the control's own space, pixels), or `None` for the rule of
    /// the engine: the hand over anything with a tapped handler or `accessibility_can_interact`.
    fn cursor(&self, local: skia_safe::Point) -> Option<Cursor> {
        self.inner().and_then(|inner| inner.cursor(local))
    }

    /// The spoken label when `accessibility_label` is empty: a label's text, a button's caption,
    /// a slider's value ("25"). Asked only when the accessibility snapshot is built (at most once
    /// a second, when a host renders it), so a value text is formatted here, not when it changes.
    fn accessibility_label(&self) -> Option<std::borrow::Cow<'_, str>> {
        self.inner().and_then(|inner| inner.accessibility_label())
    }

    /// The ARIA role when `accessibility_role` is empty (React DefaultAccessibilityRole): a
    /// checkbox, a slider, an editor. `None` = not in the accessibility tree unless the app set a
    /// role, or a default for the control's type (`Ui::default_accessibility_role`).
    fn accessibility_role(&self) -> Option<&'static str> {
        self.inner().and_then(|inner| inner.accessibility_role())
    }

    /// Tab stop and activation when `accessibility_can_interact` is not set (React
    /// DefaultAccessibilityCanInteract): false for a disabled button. `None` = the control has a
    /// tapped handler.
    fn accessibility_can_interact(&self) -> Option<bool> {
        self.inner().and_then(|inner| inner.accessibility_can_interact())
    }

    /// Takes hover when `receives_hover` is not set (DrawnUI ReceivesHoverByDefault): true for
    /// buttons, sliders, toggles, radio buttons, carousels and drawers.
    fn receives_hover(&self) -> bool {
        self.inner().is_some_and(|inner| inner.receives_hover())
    }

    /// A control that moves its content by itself (a scroll, a carousel, a drawer) says whether it
    /// is moving it now; `None` for every other control. While one moves, hover waits; it is checked
    /// again when they stop (DrawnUI PauseHover / ResumeHover).
    fn moves_content(&self) -> Option<bool> {
        self.inner().and_then(|inner| inner.moves_content())
    }

    /// aria-pressed / aria-checked when `accessibility_is_pressed` is not set: a toggle's state.
    fn accessibility_is_pressed(&self) -> Option<bool> {
        self.inner().and_then(|inner| inner.accessibility_is_pressed())
    }

    /// A range control's value, range and step (a slider, a progress bar). A screen reader's
    /// Increment / Decrement reach the control as ArrowUp / ArrowDown (`Control::key`).
    fn accessibility_value(&self) -> Option<crate::ui::AccessibilityValue> {
        self.inner().and_then(|inner| inner.accessibility_value())
    }

    /// The browser went back or forward in its history (a SkiaShell with UseBrowserHistory), for
    /// a control that listens (`Cx::listen_history`): `depth` is what the entry was pushed with
    /// (`HistoryOp::Push`), 0 for an entry the app did not push; `hash` the URL hash now.
    fn on_history(&mut self, cx: &mut GestureCx, depth: u32, hash: &str) {
        if let Some(inner) = self.inner_mut() {
            inner.on_history(cx, depth, hash)
        }
    }

    /// The laid-out text lines, for `accessibility_text_selectable` (React
    /// GetAccessibilityTextLines): the overlay renders them as real, invisible text the browser
    /// selects and copies. `scale` is pixels per point. Default: none.
    fn accessibility_text_lines(&self, scale: f32) -> Vec<crate::ui::AccessibilityTextLine> {
        self.inner().map_or_else(Vec::new, |inner| inner.accessibility_text_lines(scale))
    }

    /// Runs once per frame for a control whose properties were changed with `Dirty::APPLY`,
    /// before layout. The place to push own properties into child controls.
    fn on_props_changed(&mut self, cx: &mut Cx) {
        if let Some(inner) = self.inner_mut() {
            inner.on_props_changed(cx)
        }
    }
}

/// `Self` carries the properties part `P`, directly or through its inner chain.
pub trait Has<P>: Control {
    fn part(&self) -> &P;
    fn part_mut(&mut self) -> &mut P;
}

pub(crate) fn part<T: Control>(c: &dyn Control) -> Option<&T> {
    match (c as &dyn Any).downcast_ref() {
        Some(t) => Some(t),
        None => part(c.inner()?),
    }
}

pub(crate) fn part_mut<T: Control>(c: &mut dyn Control) -> Option<&mut T> {
    if (&*c as &dyn Any).is::<T>() {
        return (c as &mut dyn Any).downcast_mut();
    }
    part_mut(c.inner_mut()?)
}

/// Declares one properties part of a control: the struct with defaults, a builder trait (for
/// `Build<T>`) and a setter trait (for `Mut<T>`), both implemented for every `T: Has<Part>`.
///
/// ```ignore
/// props!(LabelProps, LabelBuild, LabelSet {
///     text / set_text: String = String::new(), MEASURE;
///     text_color / set_text_color: Color = Color::WHITE, DRAW;
/// });
/// ```
///
/// A control must not repeat a property name of a part it inherits.
#[macro_export]
macro_rules! props {
    ($props:ident, $build:ident, $set:ident {
        $($(#[$doc:meta])* $name:ident / $setter:ident : $ty:ty = $default:expr, $dirty:ident;)*
    }) => {
        #[derive(Clone, Debug, PartialEq)]
        pub struct $props { $($(#[$doc])* pub $name: $ty,)* }

        impl Default for $props {
            fn default() -> Self { Self { $($name: $default,)* } }
        }

        pub trait $build: Sized { $($(#[$doc])* fn $name(self, v: impl $crate::IntoProp<$ty>) -> Self;)* }

        impl<T: $crate::Has<$props>> $build for $crate::Build<T> {
            $(fn $name(mut self, v: impl $crate::IntoProp<$ty>) -> Self {
                $crate::Has::<$props>::part_mut(self.control_mut()).$name = $crate::IntoProp::into_prop(v);
                self
            })*
        }

        pub trait $set { $(fn $setter(&mut self, v: impl $crate::IntoProp<$ty>);)* }

        impl<T: $crate::Has<$props>> $set for $crate::Mut<'_, T> {
            $(fn $setter(&mut self, v: impl $crate::IntoProp<$ty>) {
                let v = $crate::IntoProp::into_prop(v);
                let part = $crate::Has::<$props>::part_mut(self.control_mut());
                if part.$name != v {
                    part.$name = v;
                    self.mark($crate::Dirty::$dirty);
                }
            })*
        }
    };
}

// ---------------------------------------------------------------- layout context

/// Given to `measure` and `arrange`. The control's own node is in the tree, minus its kind.
pub struct LayoutCx<'a> {
    pub(crate) tree: &'a mut Tree,
    pub fonts: &'a Fonts,
    /// The app state: a templated list binds its cells during layout.
    pub(crate) state: &'a dyn Any,
    pub id: ControlId,
    /// Pixels per point.
    pub scale: f32,
}

impl LayoutCx<'_> {
    pub fn base(&self) -> &Base {
        &self.tree.node(self.id).expect("control is mounted").base
    }

    /// The control's own state. Style defaults may be written here during measure.
    pub fn base_mut(&mut self) -> &mut Base {
        &mut self.tree.node_mut(self.id).expect("control is mounted").base
    }

    /// The part of the control's rect that is visible through its ancestors (their boxes and
    /// scroll offsets), in the control's own coordinates. Valid in `arrange`.
    pub fn visible_rect(&self) -> Rect {
        self.tree.visible_rect(self.id)
    }

    /// Asks for an `arrange` whenever an ancestor scrolls. For controls that realize only what is
    /// visible.
    pub fn track_viewport(&mut self) {
        if !std::mem::replace(&mut self.base_mut().tracks_viewport, true) {
            self.tree.trackers.push(self.id);
        }
    }

    /// For a scroll, right after it arranged its content: the pixels the content above the visible
    /// rows grew by in that arrange (a row turned out larger than estimated, items were inserted
    /// above). Moving the scroll offset back by it keeps the rows on screen where they are; the
    /// content is arranged for that already. 0 for a child that reports nothing.
    pub fn take_viewport_shift(&mut self, child: ControlId) -> f32 {
        self.tree.node_mut(child).map_or(0.0, |n| std::mem::take(&mut n.base.viewport_shift))
    }

    pub fn child_count(&self) -> usize {
        self.tree.children(self.id).len()
    }

    pub fn child(&self, index: usize) -> ControlId {
        self.tree.children(self.id)[index]
    }

    pub fn child_base(&self, child: ControlId) -> &Base {
        &self.tree.node(child).expect("child is mounted").base
    }

    /// Measures a child inside the given constraints (pixels). Returns its size, margin included.
    pub fn measure_child(&mut self, child: ControlId, width: f32, height: f32) -> Size {
        layout::measure(self.tree, self.fonts, self.state, child, width, height, self.scale)
    }

    /// Places a child inside `destination` (pixels) by its margin and layout options.
    pub fn arrange_child(&mut self, child: ControlId, destination: Rect) {
        layout::arrange(self.tree, self.fonts, self.state, child, destination, self.scale)
    }
}

// ---------------------------------------------------------------- paint context

/// Given to `paint`. Nodes are read-only; only caches and render matrices are written.
pub struct PaintCx<'a> {
    pub canvas: &'a Canvas,
    /// The control's drawing rect in pixels.
    pub rect: Rect,
    /// Pixels per point.
    pub scale: f32,
    pub id: ControlId,
    pub fonts: &'a Fonts,
    pub(crate) nodes: &'a [Option<Node>],
    pub(crate) animators: &'a [Animator],
    pub(crate) render: &'a mut [RenderSlot],
    pub(crate) drops: &'a mut Vec<CachedObject>,
    pub(crate) gpu: &'a mut Gpu,
    pub(crate) bakes: &'a mut crate::bakes::Bakes,
    /// A picture for a worker is recorded: no GPU texture may go into it (caches on the GPU are
    /// painted live, post renderers and backdrops are left out).
    pub(crate) offthread: bool,
    /// While an Operations cache records its picture: the surface the picture lands on.
    pub(crate) target: Option<Target<'a>>,
}

/// The surface a picture being recorded lands on (DrawnUI `DrawingContext.Surface` while an
/// Operations cache records). What is under the recorded control is on it already, because a
/// cache records while its parent paints: SkiaBackdrop and shader backgrounds copy it from there.
#[derive(Clone, Copy)]
pub(crate) struct Target<'a> {
    /// A canvas with a surface: the window's, or an Image cache's offscreen one.
    pub canvas: &'a Canvas,
    /// From the recording's coordinates to that surface's pixels.
    pub matrix: Matrix,
    /// The control whose picture is recorded.
    pub recording: ControlId,
}

impl<'a> PaintCx<'a> {
    pub(crate) fn node(&self, id: ControlId) -> Option<&'a Node> {
        let nodes: &'a [Option<Node>] = self.nodes;
        nodes.get(id.index as usize)?.as_ref().filter(|n| n.id == id)
    }

    pub fn base(&self) -> &'a Base {
        &self.node(self.id).expect("control is mounted").base
    }

    pub fn has_children(&self) -> bool {
        self.node(self.id).is_some_and(|n| !n.children.is_empty())
    }

    /// Draws one child: its cache when it has a valid one, else its content. The control's
    /// `content_offset` moves it.
    pub fn paint_child(&mut self, child: ControlId) {
        let offset = self.base().content_offset;
        if offset.is_zero() {
            return paint::render(self, child);
        }
        self.canvas.save();
        self.canvas.translate(offset);
        paint::render(self, child);
        self.canvas.restore();
    }

    /// Draws all children, lowest `z_index` first, moved by the control's `content_offset`.
    pub fn paint_children(&mut self) {
        let Some(node) = self.node(self.id) else { return };
        let children: &'a [ControlId] = &node.children;
        let offset = node.base.content_offset;
        if !offset.is_zero() {
            self.canvas.save();
            self.canvas.translate(offset);
        }
        if children.iter().all(|c| self.node(*c).is_none_or(|n| n.base.p.z_index == 0)) {
            for &child in children {
                paint::render(self, child);
            }
        } else {
            let mut sorted = children.to_vec();
            sorted.sort_by_key(|c| self.node(*c).map_or(0, |n| n.base.p.z_index));
            for child in sorted {
                paint::render(self, child);
            }
        }
        if !offset.is_zero() {
            self.canvas.restore();
        }
    }
}

// ---------------------------------------------------------------- gesture context

/// Given to `on_gesture`. The control's own node is in the tree, minus its kind.
pub struct GestureCx<'a> {
    pub(crate) tree: &'a mut Tree,
    pub id: ControlId,
    /// The gesture location in the control's own space, pixels.
    pub point: skia_safe::Point,
    /// What routing into the children needs; `None` outside the gesture router (keys, focus).
    pub(crate) routing: Option<crate::ui::Routing<'a>>,
    /// `route_children` ran: the router does not ask the children again.
    pub(crate) children_routed: bool,
}

impl<'a> GestureCx<'a> {
    /// A context without the router: `route_children` finds no children.
    pub(crate) fn new(tree: &'a mut Tree, id: ControlId, point: skia_safe::Point) -> Self {
        Self { tree, id, point, routing: None, children_routed: false }
    }
}

impl GestureCx<'_> {
    pub fn base(&self) -> &Base {
        &self.tree.node(self.id).expect("control is mounted").base
    }

    /// Routes the gesture into the children now, as the router would after this hook returned
    /// `Handled::No` (React `super.ProcessGestures`): the lock check, the `content_offset`, top-most
    /// child first, each child's hook, its children and its handlers. Returns the child that
    /// consumed it; answer `Handled::By(child)` to let it be the consumer. The router does not ask
    /// the children again after this hook, whatever it answers; with `Handled::No` the control's
    /// own handlers (tapped, long press, context menu) still run. `None` outside the router.
    pub fn route_children(&mut self, gesture: &Gesture) -> Option<ControlId> {
        let routing = self.routing.as_mut()?;
        self.children_routed = true;
        crate::ui::route_children(&mut *self.tree, routing, self.id, gesture, self.point)
    }

    /// Marks the control itself dirty (a pressed state changed its look).
    pub fn invalidate(&mut self, dirty: Dirty) {
        self.tree.invalidate(self.id, dirty)
    }

    /// Access to other controls.
    pub fn cx(&mut self) -> Cx<'_> {
        Cx { tree: self.tree }
    }

    /// The control is `Ui::focused`: keys come to it.
    pub fn is_focused(&self) -> bool {
        self.tree.focused == Some(self.id)
    }

    /// Claims the keyboard focus (an editor on its Down). Applied after the current gesture or
    /// key was routed; a tap in the same gesture does not move it away again.
    pub fn focus(&mut self) {
        self.tree.focus_request = Some(Some(self.id));
    }

    /// Gives the focus up (an editor on Escape).
    pub fn unfocus(&mut self) {
        if self.is_focused() {
            self.tree.focus_request = Some(None);
        }
    }
}
