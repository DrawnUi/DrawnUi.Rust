//! SkiaToggle: the base of SkiaSwitch, SkiaCheckbox and SkiaRadioButton. A tap flips
//! `is_toggled`, `on_toggled` reports every change (from a tap or from code), `control_style`
//! picks the look the default content is built with; a later style change rebuilds it (DrawnUI
//! RebuildDefaultContent). Colors left unset take the style's defaults (DrawnUI SetStyleDefault).

use std::any::Any;

use skia_safe::Color;

use crate::animators::{self, FrameTick};
use crate::control::{Control, GestureCx, Handled, Has, part_mut};
use crate::controls::control_style::PrebuiltControlStyle;
use crate::controls::layout::{LayoutProps, SkiaLayout};
use crate::gestures::{Gesture, GestureKind};
use crate::props;
use crate::tree::{Build, ControlId, Cx, Detached, Handle, Mut, Raw, wrong_state};
use crate::types::Dirty;

props!(ToggleProps, ToggleBuild, ToggleSet {
    /// The state. Set from code it changes the look and runs `on_toggled` like a tap does.
    is_toggled / set_is_toggled: bool = false, APPLY;
    /// The look the content is built with. Changing it on a mounted control rebuilds the content.
    control_style / set_control_style: PrebuiltControlStyle = PrebuiltControlStyle::Unset, APPLY;
    /// A switch moves its thumb over 200 ms; off, it jumps.
    is_animated / set_is_animated: bool = true, NONE;
    /// Off, taps do nothing.
    responds_to_gestures / set_responds_to_gestures: bool = true, NONE;
    /// Unset = the style's default.
    color_thumb_on / set_color_thumb_on: Option<Color> = None, APPLY;
    color_frame_on / set_color_frame_on: Option<Color> = None, APPLY;
    color_thumb_off / set_color_thumb_off: Option<Color> = None, APPLY;
    color_frame_off / set_color_frame_off: Option<Color> = None, APPLY;
});

pub(crate) type Toggled = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>, bool)>;

/// The four toggle colors, for the per-style default tables.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToggleColor {
    ThumbOn,
    FrameOn,
    ThumbOff,
    FrameOff,
}

/// The base part: an Absolute layout whose children are the look.
pub struct SkiaToggle {
    layout: SkiaLayout,
    pub p: ToggleProps,
    /// The control's own id, known from the builder: handlers and the content rebuild need it.
    pub(crate) id: ControlId,
    on_toggled: Option<Toggled>,
    /// `is_toggled` as `on_toggled` last saw it; `None` before the first apply.
    reported: Option<bool>,
    /// A change `on_toggled` has not reported yet.
    pending: Option<bool>,
    /// The style the content was built for.
    built: Option<PrebuiltControlStyle>,
    /// Size requests the style set (width, height, minimum height): a rebuild releases only these.
    pinned: [bool; 3],
}

impl SkiaToggle {
    /// The id is set by the derived control's builder once it reserved one.
    pub(crate) fn new() -> Self {
        Self {
            layout: SkiaLayout::default(),
            p: ToggleProps::default(),
            id: Handle::<SkiaToggle>::default().id(),
            on_toggled: None,
            reported: None,
            pending: None,
            built: None,
            pinned: [false; 3],
        }
    }

    /// The look that is drawn (DrawnUI UsingControlStyle).
    pub fn using_control_style(&self) -> PrebuiltControlStyle {
        self.built.unwrap_or_else(|| self.p.control_style.resolve())
    }

    /// A color as the content takes it: the app's value, else the style's default, else the
    /// SkiaToggle default (thumbs white, frame red on, dark gray off).
    pub(crate) fn color(&self, which: ToggleColor, style_default: Option<Color>) -> Color {
        let (set, base) = match which {
            ToggleColor::ThumbOn => (self.p.color_thumb_on, Color::WHITE),
            ToggleColor::FrameOn => (self.p.color_frame_on, Color::RED),
            ToggleColor::ThumbOff => (self.p.color_thumb_off, Color::WHITE),
            ToggleColor::FrameOff => (self.p.color_frame_off, Color::from_rgb(0xA9, 0xA9, 0xA9)),
        };
        set.or(style_default).unwrap_or(base)
    }

    /// `is_toggled` as `on_toggled` last saw it; `None` before the first apply.
    pub(crate) fn reported(&self) -> Option<bool> {
        self.reported
    }

    /// True when the content must be built (first time) or built again (another style).
    pub(crate) fn needs_content(&self) -> bool {
        self.built != Some(self.p.control_style.resolve())
    }

    /// Replaces the content with what `build` makes for the current style (DrawnUI
    /// CreateDefaultContent / RebuildDefaultContent). Size requests the previous style pinned go back
    /// to unset; the app's own requests stay.
    pub(crate) fn rebuild(&mut self, cx: &mut Cx, build: impl FnOnce(PrebuiltControlStyle) -> Vec<Detached>) {
        let style = self.p.control_style.resolve();
        let old: Vec<ControlId> = cx.tree.children(self.id).to_vec();
        for child in old {
            cx.tree.remove_now(child);
        }
        if let Some(node) = cx.tree.node_mut(self.id) {
            let p = &mut node.base.p;
            if self.pinned[0] {
                p.width_request = -1.0;
            }
            if self.pinned[1] {
                p.height_request = -1.0;
            }
            if self.pinned[2] {
                p.minimum_height_request = -1.0;
            }
        }
        self.pinned = [false; 3];
        for child in build(style) {
            let id = cx.add_child(self.id, child);
            if let Some(node) = cx.tree.node_mut(id) {
                node.part = true;
            }
        }
        self.built = Some(style);
    }

    /// DrawnUI SetDefaultContentSize: the style's size, only where the app left the request unset.
    pub(crate) fn pin_size(&mut self, cx: &mut Cx, width: f32, height: f32) {
        let Some(node) = cx.tree.node_mut(self.id) else { return };
        let p = &mut node.base.p;
        if p.width_request < 0.0 {
            p.width_request = width;
            self.pinned[0] = true;
        }
        if p.height_request < 0.0 {
            p.height_request = height;
            self.pinned[1] = true;
        }
        cx.tree.invalidate(self.id, Dirty::MEASURE);
    }

    /// DrawnUI SetDefaultMinimumContentSize (height only), where the app left it unset.
    pub(crate) fn pin_minimum_height(&mut self, cx: &mut Cx, height: f32) {
        let Some(node) = cx.tree.node_mut(self.id) else { return };
        if node.base.p.minimum_height_request < 0.0 {
            node.base.p.minimum_height_request = height;
            self.pinned[2] = true;
            cx.tree.invalidate(self.id, Dirty::MEASURE);
        }
    }

    /// Whether `is_toggled` changed since the last call. When it did (not at the first call),
    /// `on_toggled` runs at the start of the next frame, before the observers.
    pub(crate) fn take_change(&mut self, cx: &mut Cx) -> bool {
        let value = self.p.is_toggled;
        let changed = self.reported.is_some_and(|reported| reported != value);
        self.reported = Some(value);
        if changed && self.on_toggled.is_some() && self.pending.replace(value).is_none() {
            animators::start_frame(cx.tree, self.id, fire);
        }
        changed
    }

    /// The tap of every toggle: flips the state; `on_tapped` runs too.
    pub(crate) fn tap(&mut self, cx: &mut GestureCx) -> Handled {
        if !self.p.responds_to_gestures {
            return Handled::No;
        }
        self.p.is_toggled = !self.p.is_toggled;
        cx.invalidate(Dirty::APPLY);
        Handled::Tapped
    }
}

/// The frame animator that runs `on_toggled` with the app state: the node leaves its slot
/// meanwhile, like for a tapped handler, so the handler can reach the rest of the tree.
fn fire(id: ControlId, _time_ms: f64, state: &mut dyn Any, cx: &mut Cx<'_>) -> FrameTick {
    let mut tick = FrameTick { keep: false, state_touched: false };
    let Some(mut node) = cx.tree.take(id) else { return tick };
    let mut queue = Vec::new();
    if let Some(control) = node.kind.as_deref_mut() {
        let due = part_mut::<SkiaToggle>(control).and_then(|t| Some((t.pending.take()?, t.on_toggled.take()?)));
        if let Some((value, mut handler)) = due {
            handler(Raw { id, control, base: &mut node.base, queue: &mut queue }, state, &mut Cx { tree: cx.tree }, value);
            if let Some(toggle) = part_mut::<SkiaToggle>(control) {
                toggle.on_toggled = Some(handler);
            }
            tick.state_touched = true;
        }
    }
    cx.tree.put_back(node);
    cx.tree.queue.append(&mut queue);
    tick
}

impl Has<ToggleProps> for SkiaToggle {
    fn part(&self) -> &ToggleProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut ToggleProps {
        &mut self.p
    }
}

impl Has<LayoutProps> for SkiaToggle {
    fn part(&self) -> &LayoutProps {
        &self.layout.p
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        &mut self.layout.p
    }
}

impl Control for SkiaToggle {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }

    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        match gesture.kind {
            GestureKind::Tapped => self.tap(cx),
            _ => Handled::No,
        }
    }

    /// A tab stop while it takes taps (React DefaultAccessibilityCanInteract).
    fn accessibility_can_interact(&self) -> Option<bool> {
        Some(self.p.responds_to_gestures)
    }

    /// aria-checked: the state.
    fn accessibility_is_pressed(&self) -> Option<bool> {
        Some(self.p.is_toggled)
    }
}

impl<T: Has<ToggleProps>> Build<T> {
    /// Runs after `is_toggled` changed: by a tap, or set from code. `value` is the new state. It
    /// runs at the start of the frame after the change, before the observers (DrawnUI Toggled).
    pub fn on_toggled<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, T>, &mut S, &mut Cx<'_>, bool) + 'static) -> Self {
        let toggle: &mut SkiaToggle = part_mut(self.control_mut()).expect("the control embeds a SkiaToggle");
        toggle.on_toggled = Some(Box::new(move |me, state, cx, value| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
            f(&mut me.typed(), state, cx, value)
        }));
        self
    }
}
