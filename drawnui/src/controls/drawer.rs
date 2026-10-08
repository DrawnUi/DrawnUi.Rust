//! SkiaDrawer (React SkiaDrawer, DrawnUI SkiaDrawer): a panel that slides in from an edge
//! (`direction`), `header_size` points of it staying visible when closed (or it travels
//! `amplitude_size`). A drag moves it, a release snaps it open or closed by where the finger left
//! it and how fast; `is_open` drives and reports the state. It moves itself with its
//! `translation_x` / `translation_y`, so the parent aligns it to its edge (vertical options End
//! for `FromBottom`, and so on).

use std::any::Any;

use skia_safe::{Contains, Point, Size};

use crate::animators::{self, FrameTick};
use crate::control::{Control, GestureCx, Handled, Has, LayoutCx, part_mut};
use crate::controls::layout::{LayoutProps, SkiaLayout};
use crate::controls::scroll::fire;
use crate::controls::snapping_layout::{Snapping, SnappingProps, Tuning, dist, near};
use crate::gestures::{Gesture, GestureKind};
use crate::tree::{Build, Container, ControlId, Cx, Mut, Raw, Tree, wrong_state};
use crate::types::LayoutOptions;
use crate::props;

/// The edge a drawer comes from (DrawnUI DrawerDirection).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DrawerDirection {
    #[default]
    FromBottom,
    FromTop,
    FromLeft,
    FromRight,
}

props!(DrawerProps, DrawerBuild, DrawerSet {
    direction / set_direction: DrawerDirection = DrawerDirection::FromBottom, MEASURE;
    /// Points that stay visible when closed.
    header_size / set_header_size: f32 = 0.0, MEASURE;
    /// Points it travels between open and closed; -1 = its size less the header.
    amplitude_size / set_amplitude_size: f32 = -1.0, MEASURE;
    /// An open drawer closes when a press lands outside it (React: only a press it is given).
    auto_close / set_auto_close: bool = false, NONE;
    /// Open or closed. Set by the app, the drawer goes there (animated once it was laid out); a
    /// drag sets it too.
    is_open / set_is_open: bool = false, APPLY;
});

/// A handler that gets the drawer as `me`, the app state untyped, and the open state.
pub(crate) type FlagHandler = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>, bool)>;

pub struct SkiaDrawer {
    /// The children (React: SnappingLayout is a SkiaLayout).
    layout: SkiaLayout,
    pub p: DrawerProps,
    pub sp: SnappingProps,
    s: Snapping,
    id: Option<ControlId>,
    /// `is_open` as the drawer acts on it (React isOpen).
    open: bool,
    /// Size at the last arrange, pixels: another one lays the snap points out again.
    size: Size,
    was_drawn: bool,
    panning_offset: Point,
    child_was_tapped: bool,
    had_down: bool,
    ticking: bool,
    moved: bool,
    /// `on_is_open_changed` and `on_state_transition_complete` are due, with the state.
    open_changed: Option<bool>,
    transition_complete: Option<bool>,
    /// Set by `on_is_open_changed`, or directly by code that holds the app state untyped (the shell).
    pub(crate) on_is_open_changed: Option<FlagHandler>,
    pub(crate) on_state_transition_complete: Option<FlagHandler>,
}

impl Default for SkiaDrawer {
    fn default() -> Self {
        Self {
            layout: SkiaLayout::default(),
            p: DrawerProps::default(),
            sp: SnappingProps::default(),
            s: Snapping::default(),
            id: None,
            open: false,
            size: Size::default(),
            was_drawn: false,
            panning_offset: Point::default(),
            child_was_tapped: false,
            had_down: false,
            ticking: false,
            moved: false,
            open_changed: None,
            transition_complete: None,
            on_is_open_changed: None,
            on_state_transition_complete: None,
        }
    }
}

impl SkiaDrawer {
    /// A drawer from the bottom, filling the width.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaDrawer> {
        Build::new(SkiaDrawer::default()).horizontal_options(LayoutOptions::Fill)
    }

    pub fn is_open(&self) -> bool {
        self.open
    }
    /// On the way between open and closed, or dragged (React InTransition).
    pub fn in_transition(&self) -> bool {
        self.s.in_transition
    }
    /// The translation it has, points: 0 open (React CurrentPosition).
    pub fn current_position(&self) -> Point {
        self.s.position
    }
    /// Open, then closed, points (React SnapPoints).
    pub fn snap_points(&self) -> &[Point] {
        &self.s.snap_points
    }

    fn horizontal(&self) -> bool {
        matches!(self.p.direction, DrawerDirection::FromLeft | DrawerDirection::FromRight)
    }

    fn tuning(&self) -> Tuning {
        // React GetAutoVelocity: 1500 points per second along the axis, whatever the scale.
        Tuning { auto_velocity: Some(1500.0), ..Tuning::BASE }
    }

    fn scroll_to_offset(&mut self, target: Point, velocity: Point, animate: bool) {
        let tuning = self.tuning();
        if self.s.scroll_to_offset(target, velocity, animate, &self.sp, &tuning) {
            self.moved = true;
            self.update_reported_position();
        }
    }

    /// React OffsetToHide: the translation that hides all but the header.
    fn offset_to_hide(&self) -> Point {
        let (w, h) = (self.s.size.x, self.s.size.y);
        let travel = |size: f32| if self.p.amplitude_size >= 0.0 { self.p.amplitude_size } else { size - self.p.header_size };
        match self.p.direction {
            DrawerDirection::FromLeft => Point::new(-travel(w), 0.0),
            DrawerDirection::FromRight => Point::new(travel(w), 0.0),
            DrawerDirection::FromTop => Point::new(0.0, -travel(h)),
            DrawerDirection::FromBottom => Point::new(0.0, travel(h)),
        }
    }

    /// React ApplyOptions: the anchors are open and hidden; it goes to the one of `is_open`.
    fn apply_options(&mut self) {
        self.s.snap_points = vec![Point::default(), self.offset_to_hide()];
        self.s.bounds = self.s.bounds_from_snap_points();
        self.s.snap = Point::new(-1.0, -1.0);
        let target = self.s.snap_points[if self.open { 0 } else { 1 }];
        self.scroll_to_offset(target, Point::default(), self.sp.animated && self.was_drawn);
    }

    /// The React IsOpen setter.
    fn set_open(&mut self, open: bool) {
        if self.open == open {
            return;
        }
        (self.open, self.p.is_open, self.open_changed) = (open, open, Some(open));
        if self.s.snap_points.len() == 2 {
            let target = self.s.snap_points[if open { 0 } else { 1 }];
            self.scroll_to_offset(target, Point::default(), self.sp.animated && self.was_drawn);
        }
    }

    /// React ReportFromSnap: open unless it rests on or goes to the hidden anchor.
    fn report_from_snap(&mut self) {
        let Some(&hidden) = self.s.snap_points.get(1) else { return };
        let open = !near(hidden, self.s.snap);
        if open != self.open {
            (self.open, self.p.is_open, self.open_changed) = (open, open, Some(open));
        }
    }

    /// React UpdateReportedPosition: not while it moves.
    fn update_reported_position(&mut self) {
        if !self.s.in_transition {
            self.report_from_snap();
        }
    }

    /// React InTransition, with SkiaDrawer.CheckTransitionEnded: when a transition ends the state
    /// it reached is reported first, then StateTransitionComplete.
    fn update_transition(&mut self) {
        let ended = self.s.transition_ended();
        if ended && self.s.in_transition {
            self.report_from_snap();
            self.transition_complete = Some(self.open);
        }
        self.s.in_transition = !ended;
    }

    fn apply_position(&mut self, position: Point) {
        self.s.position = position;
        self.moved = true;
        self.update_reported_position();
    }

    fn start_ticking(&mut self, tree: &mut Tree, id: ControlId) {
        if !std::mem::replace(&mut self.ticking, true) {
            animators::start_frame(tree, id, tick);
        }
    }

    /// React resetPan.
    fn reset_pan(&mut self, translation: Point) {
        (self.s.is_user_focused, self.s.is_user_panning, self.child_was_tapped) = (true, false, false);
        self.s.stop();
        self.s.accumulator.clear();
        self.panning_offset = translation;
    }
}

/// One frame: the snap moves the drawer, the transition state follows, then the handlers run.
fn tick(id: ControlId, time_ms: f64, state: &mut dyn Any, cx: &mut Cx<'_>) -> FrameTick {
    let mut tick = FrameTick { keep: false, state_touched: false };
    let Some(mut me) = cx.tree.find_mut::<SkiaDrawer>(id) else { return tick };
    let d = me.control_mut();
    if let Some((position, done)) = d.s.animate(time_ms) {
        d.apply_position(position);
        if done {
            // React OnAnimationStopped.
            d.update_transition();
            d.update_reported_position();
        }
    }
    // React SnappingLayout.Render.
    d.update_transition();
    let (moved, position) = (std::mem::take(&mut d.moved), d.s.position);
    let (open_changed, complete) = (d.open_changed.take(), d.transition_complete.take());
    if moved {
        me.set_translation_x(position.x);
        me.set_translation_y(position.y);
    }
    if let Some(open) = open_changed {
        tick.state_touched |= run(cx, id, state, |d| &mut d.on_is_open_changed, open);
    }
    if let Some(open) = complete {
        tick.state_touched |= run(cx, id, state, |d| &mut d.on_state_transition_complete, open);
    }
    let Some(mut me) = cx.tree.find_mut::<SkiaDrawer>(id) else { return tick };
    let d = me.control_mut();
    let pending = d.moved || d.open_changed.is_some() || d.transition_complete.is_some();
    d.ticking = d.s.is_animating() || pending;
    tick.keep = d.ticking;
    tick
}

/// Runs one handler with the drawer as `me`, then puts it back, unless it set another one.
fn run(cx: &mut Cx<'_>, id: ControlId, state: &mut dyn Any, slot: fn(&mut SkiaDrawer) -> &mut Option<FlagHandler>, value: bool) -> bool {
    let Some(mut f) = cx.tree.find_mut::<SkiaDrawer>(id).and_then(|mut me| slot(me.control_mut()).take()) else { return false };
    fire(cx, id, |me, cx| f(me, &mut *state, cx, value));
    if let Some(mut me) = cx.tree.find_mut::<SkiaDrawer>(id) {
        let slot = slot(me.control_mut());
        if slot.is_none() {
            *slot = Some(f);
        }
    }
    true
}

impl Has<DrawerProps> for SkiaDrawer {
    fn part(&self) -> &DrawerProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut DrawerProps {
        &mut self.p
    }
}

impl Has<SnappingProps> for SkiaDrawer {
    fn part(&self) -> &SnappingProps {
        &self.sp
    }
    fn part_mut(&mut self) -> &mut SnappingProps {
        &mut self.sp
    }
}

impl Has<LayoutProps> for SkiaDrawer {
    fn part(&self) -> &LayoutProps {
        &self.layout.p
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        &mut self.layout.p
    }
}

impl Container for SkiaDrawer {}

impl Control for SkiaDrawer {
    fn receives_hover(&self) -> bool {
        true
    }
    fn moves_content(&self) -> Option<bool> {
        Some(self.s.in_transition)
    }
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }

    /// `is_open` set by the app: the drawer goes there.
    fn on_props_changed(&mut self, cx: &mut Cx) {
        self.set_open(self.p.is_open);
        self.s.started_at(cx.tree.time_ms);
        if let Some(id) = self.id {
            self.start_ticking(cx.tree, id);
        }
    }

    /// Its children as a layout; a new size lays the anchors out again.
    fn arrange(&mut self, cx: &mut LayoutCx) {
        self.layout.arrange(cx);
        let (scale, id, rect) = (cx.scale, cx.id, cx.base().rect);
        if self.id.is_none() {
            // The state the drawer was built with.
            self.open = self.p.is_open;
        }
        self.id = Some(id);
        (self.s.scale, self.s.size) = (scale, Point::new(rect.width() / scale, rect.height() / scale));
        if rect.size() != self.size {
            self.size = rect.size();
            self.apply_options();
        }
        self.was_drawn = true;
        // Placed for its position already: the first frame shows it there.
        let position = self.s.position;
        let base = cx.base_mut();
        (base.p.translation_x, base.p.translation_y) = (position.x, position.y);
        self.moved = false;
        self.start_ticking(cx.tree, id);
    }

    /// React SkiaDrawer.ProcessGestures, one pointer: the children first while it does not pan; a
    /// drag along its axis moves it, the release snaps it open or closed.
    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        let kind = gesture.kind;
        let scale = self.s.scale;
        // `cx.point` is in its own untransformed space: inside its rect is inside the drawer as drawn.
        if !self.had_down && kind != GestureKind::Up && !cx.base().rect.contains(cx.point) {
            if self.p.auto_close && self.open && !self.s.in_transition {
                self.set_open(false);
                self.start_ticking(cx.tree, cx.id);
            }
            return Handled::No;
        }
        let (mut passed, mut child) = (false, None);
        if matches!(kind, GestureKind::Up | GestureKind::Tapped) || !self.s.is_user_panning || !self.sp.responds_to_gestures {
            (passed, child) = (true, cx.route_children(gesture));
            if let Some(child) = child
                && kind != GestureKind::Up
            {
                self.child_was_tapped |= kind == GestureKind::Tapped;
                return Handled::By(child);
            }
        }
        if !self.sp.responds_to_gestures {
            return child.map_or(Handled::No, Handled::By);
        }
        let translation = cx.base().p.translation_x;
        let translation = Point::new(translation, cx.base().p.translation_y);
        let mut consumed = false;
        match kind {
            GestureKind::Tapped | GestureKind::LongPressing => consumed = true,
            GestureKind::Down => {
                self.had_down = true;
                self.reset_pan(translation);
            }
            GestureKind::Panning => 'pan: {
                if !self.had_down {
                    return Handled::No;
                }
                let horizontal = self.horizontal();
                let (dx, dy) = (gesture.delta.x, gesture.delta.y);
                // Dragged further open at the open edge: no rubber band there (C# lockBounce).
                let at_open = self.s.snap_points.first().is_some_and(|open| near(*open, self.s.position));
                let lock_bounce = at_open
                    && match self.p.direction {
                        DrawerDirection::FromLeft => dx > 0.0,
                        DrawerDirection::FromRight => dx < 0.0,
                        DrawerDirection::FromBottom => dy < 0.0,
                        DrawerDirection::FromTop => dy > 0.0,
                    };
                if !self.s.is_user_focused {
                    self.reset_pan(translation);
                    self.panning_offset -= gesture.delta * (1.0 / scale);
                }
                let (mut x, mut y) = (self.panning_offset.x + dx / scale, self.panning_offset.y + dy / scale);
                if !self.s.is_user_panning {
                    let (tx, ty) = (gesture.total.x.abs(), gesture.total.y.abs());
                    let (main_horizontal, main_vertical) = (tx > ty * 0.9, ty > tx * 0.9);
                    if self.sp.ignore_wrong_direction && ((horizontal && !main_horizontal) || (!horizontal && !main_vertical)) {
                        break 'pan;
                    }
                    if if horizontal { tx < scale } else { ty < scale } {
                        break 'pan;
                    }
                    self.s.is_user_panning = true;
                }
                let velocity = gesture.velocity * (1.0 / scale);
                if horizontal {
                    self.s.accumulator.capture(Point::new(velocity.x, 0.0), gesture.time_ms);
                    y = 0.0;
                } else {
                    self.s.accumulator.capture(Point::new(0.0, velocity.y), gesture.time_ms);
                    x = 0.0;
                }
                self.panning_offset = Point::new(x, y);
                let clamped = self.s.clamp(x, y, self.sp.bounces && !lock_bounce, self.sp.rubber_effect);
                if !self.sp.bounces && lock_bounce && clamped.x.abs() <= 1.0 && clamped.y.abs() <= 1.0 {
                    // Open and pushed further: a scroll around it may take the pan.
                    self.s.is_user_panning = false;
                    return Handled::No;
                }
                self.apply_position(clamped);
                consumed = true;
            }
            GestureKind::Up => 'up: {
                self.had_down = false;
                if self.child_was_tapped || !self.s.is_user_panning {
                    break 'up;
                }
                // React reads the clock while it processes the Up: the frame time here.
                let velocity = self.s.accumulator.final_velocity(cx.tree.time_ms, 3000.0);
                let velocity = if self.horizontal() { Point::new(velocity.x, 0.0) } else { Point::new(0.0, velocity.y) };
                self.s.snap = self.s.position;
                // React ScrollToNearestAnchor (the base one).
                let location = self.s.position;
                let target = self.s.select_next_anchor(self.s.nearest_anchor(location), velocity);
                if dist(location, target) >= 0.5 {
                    self.scroll_to_offset(target, velocity, self.s.size.y > 0.0);
                } else {
                    self.update_reported_position();
                }
                (self.s.is_user_panning, self.s.is_user_focused) = (false, false);
                consumed = true;
            }
            _ => {}
        }
        self.start_ticking(cx.tree, cx.id);
        if consumed {
            return if kind == GestureKind::Tapped { Handled::Tapped } else { Handled::Yes };
        }
        if let Some(child) = child {
            return Handled::By(child);
        }
        if self.s.is_user_panning {
            return if kind == GestureKind::Up { Handled::No } else { Handled::Yes };
        }
        if !passed {
            return cx.route_children(gesture).map_or(Handled::No, Handled::By);
        }
        Handled::No
    }
}

fn drawer_part<T: Control>(control: &mut T) -> &mut SkiaDrawer {
    part_mut(control).expect("the control embeds a SkiaDrawer")
}

fn flag_handler<T: Control, S: Any>(mut f: impl FnMut(&mut Mut<'_, T>, &mut S, &mut Cx<'_>, bool) + 'static) -> FlagHandler {
    Box::new(move |me, state, cx, value| {
        let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
        f(&mut me.typed(), state, cx, value)
    })
}

impl<T: Has<DrawerProps>> Build<T> {
    /// Runs when the drawer opens or closes, by a drag or by code (React IsOpenChanged).
    pub fn on_is_open_changed<S: Any>(mut self, f: impl FnMut(&mut Mut<'_, T>, &mut S, &mut Cx<'_>, bool) + 'static) -> Self {
        drawer_part(self.control_mut()).on_is_open_changed = Some(flag_handler(f));
        self
    }

    /// Runs when the drawer came to rest open (true) or closed (React StateTransitionComplete).
    pub fn on_state_transition_complete<S: Any>(mut self, f: impl FnMut(&mut Mut<'_, T>, &mut S, &mut Cx<'_>, bool) + 'static) -> Self {
        drawer_part(self.control_mut()).on_state_transition_complete = Some(flag_handler(f));
        self
    }
}

impl Mut<'_, SkiaDrawer> {
    /// React Open.
    pub fn open(&mut self) {
        self.set_is_open(true);
    }
    /// React Close.
    pub fn close(&mut self) {
        self.set_is_open(false);
    }
}
