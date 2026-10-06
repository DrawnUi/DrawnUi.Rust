//! SkiaCarousel (React SkiaCarousel, DrawnUI SkiaCarousel): every child is a slide as large as
//! the carousel, laid along its axis; a swipe or a release snaps to a slide by where the finger
//! left it and how fast. `sides_offset` lets the neighbors peek in, `spacing` separates slides,
//! `selected_index` drives and reports the slide, `is_looped` wraps around through virtual anchors
//! one step before the first slide and after the last, `dynamic_size` sizes an auto-sized carousel
//! by the selected slide. Children, or `items(count, template, bind)` for templated slides.
//!
//! Slides are arranged once at their places; moving the carousel is its `content_offset`, so no
//! layout runs for a frame of a swipe or a snap. A looped carousel arranges again only when a slide
//! at an end changes the side it is drawn on.

use std::any::Any;

use skia_safe::{Point, Rect, Size};

use crate::animators::{self, FrameTick, easing};
use crate::control::{Control, GestureCx, Handled, Has, LayoutCx, PaintCx, part_mut};
use crate::controls::layout::{LayoutProps, SkiaLayout};
use crate::controls::scroll::fire;
use crate::controls::snapping_layout::{Duration, Snapping, SnappingProps, Tuning, dist, near};
use crate::gestures::{Gesture, GestureKind};
use crate::tree::{Build, Container, ControlId, Cx, Mut, Raw, Tree, wrong_state};
use crate::types::{Dirty, LayoutOptions, Thickness};
use crate::{layout, props};

props!(CarouselProps, CarouselBuild, CarouselSet {
    /// Slides go down instead of across.
    is_vertical / set_is_vertical: bool = false, MEASURE;
    /// Points on both sides of a slide, along the axis, where the neighbors peek in.
    sides_offset / set_sides_offset: f32 = 0.0, MEASURE;
    /// From the last slide on to the first and back.
    is_looped / set_is_looped: bool = false, MEASURE;
    /// Neighbors that are off screen but next to it are drawn too (React: they are painted, so
    /// their images load; here a slide loads when it is measured, and every slide is).
    preload_neighboors / set_preload_neighboors: bool = true, DRAW;
    /// A carousel without a size along an axis takes the size of the selected slide, not of the
    /// largest one, and is measured again when the slide changes.
    dynamic_size / set_dynamic_size: bool = false, MEASURE;
    /// Multiplies the speed of a snap: the velocity it starts with, the stiffness of its spring.
    swipe_speed / set_swipe_speed: f32 = 1.0, NONE;
    /// Milliseconds a whole slide takes when a snap does not bounce; 0 = from the velocity.
    linear_speed_ms / set_linear_speed_ms: f32 = 0.0, NONE;
    /// The slide shown. Set by the app, the carousel goes there (animated); a swipe sets it too.
    selected_index / set_selected_index: usize = 0, APPLY;
});

type Handler<V> = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>, V)>;
type IndexHandler = Handler<usize>;
type FlagHandler = Handler<bool>;

/// What the carousel tells the app, in the order it happened.
#[derive(Clone, Copy)]
enum Event {
    Index(usize),
    Transition(bool),
    Appearing(usize),
    Disappearing(usize),
}

/// A virtual anchor of a looped carousel (React SnapPoint): -1 one step before the first slide,
/// -2 one step after the last, else the slide.
#[derive(Clone, Copy)]
struct Anchor {
    id: i32,
    at: Point,
}

pub struct SkiaCarousel {
    /// The children or the templated cells, the padding and the spacing (React: SnappingLayout is a
    /// SkiaLayout).
    layout: SkiaLayout,
    pub p: CarouselProps,
    pub sp: SnappingProps,
    s: Snapping,
    id: Option<ControlId>,
    /// The index the carousel acts on (React selectedIndex); `p.selected_index` follows it.
    selected: usize,
    last_index: Option<usize>,
    /// A `scroll_to` from code: the index, and whether it animates.
    order: Option<(usize, bool)>,
    /// Slides at the last arrange.
    count: usize,
    /// Slide size, points (C# CellSize).
    cell: Size,
    /// What the snap points were made for: axis, spacing, sides offset, slide length.
    key: Option<(bool, f32, f32, f32)>,
    last_looped: bool,
    /// Per slide: on screen (ItemAppearing was sent), and drawn.
    visible: Vec<bool>,
    shown: Vec<bool>,
    /// The first slide is drawn after the last one, the last before the first (looped).
    wrapped: [bool; 2],
    panning_offset: Point,
    panning_start: Point,
    wrong_direction: bool,
    snap_if_no_pan_on_up: bool,
    had_down: bool,
    /// The frame animator is registered.
    ticking: bool,
    /// The position changed since the last tick: the content offset follows.
    moved: bool,
    events: Vec<Event>,
    on_selected_index_changed: Option<IndexHandler>,
    on_transition_changed: Option<FlagHandler>,
    on_item_appearing: Option<IndexHandler>,
    on_item_disappearing: Option<IndexHandler>,
}

impl Default for SkiaCarousel {
    fn default() -> Self {
        let mut layout = SkiaLayout::default();
        // React / C#: a carousel has no spacing unless given one.
        layout.p.spacing = 0.0;
        Self {
            layout,
            p: CarouselProps::default(),
            sp: SnappingProps::default(),
            s: Snapping::default(),
            id: None,
            selected: 0,
            last_index: None,
            order: None,
            count: 0,
            cell: Size::default(),
            key: None,
            last_looped: false,
            visible: Vec::new(),
            shown: Vec::new(),
            wrapped: [false; 2],
            panning_offset: Point::default(),
            panning_start: Point::default(),
            wrong_direction: false,
            snap_if_no_pan_on_up: false,
            had_down: false,
            ticking: false,
            moved: false,
            events: Vec::new(),
            on_selected_index_changed: None,
            on_transition_changed: None,
            on_item_appearing: None,
            on_item_disappearing: None,
        }
    }
}

impl SkiaCarousel {
    /// A horizontal carousel filling the width, clipped to its bounds.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaCarousel> {
        Build::new(SkiaCarousel::default()).horizontal_options(LayoutOptions::Fill).is_clipped_to_bounds(true)
    }

    /// The slide it rests on or travels to.
    pub fn selected_index(&self) -> usize {
        self.selected
    }
    /// The slide selected before (React LastIndex).
    pub fn last_index(&self) -> Option<usize> {
        self.last_index
    }
    /// Number of slides.
    pub fn children_count(&self) -> usize {
        self.count
    }
    /// Between two slides, or on the way to one (React InTransition).
    pub fn in_transition(&self) -> bool {
        self.s.in_transition
    }
    /// Where the slides are, points: 0 at the first slide, negative further on (React CurrentPosition).
    pub fn current_position(&self) -> Point {
        self.s.position
    }
    /// The anchors, one per slide, points (React SnapPoints).
    pub fn snap_points(&self) -> &[Point] {
        &self.s.snap_points
    }
    pub fn is_user_panning(&self) -> bool {
        self.s.is_user_panning
    }

    fn along(&self, p: Point) -> f32 {
        if self.p.is_vertical { p.y } else { p.x }
    }

    fn max_index(&self) -> usize {
        self.count.saturating_sub(1)
    }

    fn looped(&self) -> bool {
        self.p.is_looped && self.s.snap_points.len() > 1
    }

    /// React ApplyLoopedLogic.
    fn apply_looped_logic(&self) -> bool {
        self.looped() && self.sp.animated && self.can_animate()
    }

    fn can_animate(&self) -> bool {
        self.s.size.y > 0.0
    }

    /// Slide size plus spacing less both sides offsets (React Step).
    fn step(&self) -> f32 {
        let size = if self.p.is_vertical { self.cell.height } else { self.cell.width };
        size + self.layout.p.spacing - 2.0 * self.p.sides_offset
    }

    fn tuning(&self) -> Tuning {
        let cell = Point::new(self.cell.width, self.cell.height);
        Tuning {
            velocity_scale: self.p.swipe_speed / 2.0,
            stiffness_scale: self.p.swipe_speed,
            spring_velocity_scale: self.p.swipe_speed,
            auto_velocity: None,
            duration: Duration::Carousel { swipe_speed: self.p.swipe_speed, linear_speed_ms: self.p.linear_speed_ms, cell, vertical: self.p.is_vertical },
            easing: easing::sin_out,
        }
    }

    /// React ScrollToOffset, then the index of where it goes is reported.
    fn scroll_to_offset(&mut self, target: Point, velocity: Point, animate: bool) {
        let (tuning, was) = (self.tuning(), self.s.in_transition);
        if self.s.scroll_to_offset(target, velocity, animate, &self.sp, &tuning) {
            if self.s.in_transition && !was {
                self.events.push(Event::Transition(true));
            }
            self.moved = true;
            self.update_reported_position();
        }
    }

    // ------------------------------------------------------------ looped: virtual anchors

    fn virtual_anchors(&self) -> impl Iterator<Item = Anchor> + '_ {
        let s = &self.s.snap_points;
        let (n, d) = (s.len(), s[1] - s[0]);
        [Anchor { id: -1, at: s[0] - d }, Anchor { id: -2, at: s[n - 1] + d }]
            .into_iter()
            .chain(s.iter().enumerate().map(|(i, p)| Anchor { id: i as i32, at: *p }))
    }

    fn virtual_anchor(&self, id: i32) -> Point {
        self.virtual_anchors().find(|a| a.id == id).expect("virtual anchor").at
    }

    /// The virtual anchor nearest to `current`; the first of equals wins (React GetVirtualAnchor).
    fn nearest_virtual(&self, current: Point) -> Anchor {
        let mut best = (Anchor { id: -1, at: current }, f32::INFINITY);
        for a in self.virtual_anchors() {
            let d = dist(a.at, current);
            if d < best.1 {
                best = (a, d);
            }
        }
        best.0
    }

    /// React FindNearestAnchorInternal: a virtual anchor stands for the real slide at the other end.
    fn nearest_anchor_internal(&self, current: Point) -> Point {
        if !self.looped() {
            return self.s.nearest_anchor(current);
        }
        let s = &self.s.snap_points;
        match self.nearest_virtual(current) {
            Anchor { id: -1, .. } => s[s.len() - 1],
            Anchor { id: -2, .. } => s[0],
            a => a.at,
        }
    }

    /// React SkiaCarousel.SelectNextAnchor: the base choice, and at an end of a looped carousel the
    /// virtual anchor beyond it when the finger went that way.
    fn select_next_anchor(&self, origin: Point, velocity: Point) -> Point {
        let base = self.s.select_next_anchor(origin, velocity);
        if !self.apply_looped_logic() {
            return base;
        }
        let s = &self.s.snap_points;
        let mut origin_snap = self.nearest_anchor_internal(origin);
        let mut origin_index = s.iter().position(|p| *p == origin_snap);
        if origin_index.is_none() {
            origin_snap = self.s.nearest_anchor(origin);
            origin_index = s.iter().position(|p| *p == origin_snap);
        }
        if origin_snap != base {
            return base;
        }
        let step = (self.along(s[1]) - self.along(s[0])).abs();
        let moved = self.along(self.s.position) - self.along(self.panning_start);
        let mut direction = crate::controls::snapping_layout::sign(self.along(velocity));
        if direction == 0.0 {
            // Released without speed: the pan counts once it covered `snap_distance_ratio` of a step.
            let needed = if step > 0.0 { step * self.sp.snap_distance_ratio } else { 0.0 };
            direction = if step <= 0.0 || moved.abs() < needed { 0.0 } else { crate::controls::snapping_layout::sign(moved) };
        }
        if direction == 0.0 || (velocity.is_zero() && step > 0.0 && moved.abs() < step * 0.5) {
            return base;
        }
        match (direction < 0.0, origin_index) {
            (true, Some(i)) if i == self.max_index() => self.virtual_anchor(-2),
            (false, Some(0)) => self.virtual_anchor(-1),
            _ => base,
        }
    }

    /// React SkiaCarousel.ScrollToNearestAnchor: under 100 points per second counts as no speed.
    fn scroll_to_nearest_anchor(&mut self, location: Point, velocity: Point) {
        if self.s.snap_points.is_empty() {
            return;
        }
        let keep = |v: f32| if v.abs() < 100.0 { 0.0 } else { v };
        let velocity = Point::new(keep(velocity.x), keep(velocity.y));
        let origin = self.nearest_anchor_internal(location);
        let target = self.select_next_anchor(origin, velocity);
        if dist(location, target) >= 0.5 {
            self.scroll_to_offset(target, velocity, self.can_animate());
        } else {
            self.update_reported_position();
        }
    }

    /// React FixIndex: a virtual anchor becomes the real slide, the position keeps its distance to
    /// it; the same picture, the wrapped slide is drawn at its own place again.
    fn fix_position(&mut self) {
        if !self.looped() || !matches!(self.nearest_virtual(self.s.snap).id, -1 | -2) || self.selected >= self.s.snap_points.len() {
            return;
        }
        let snap = self.nearest_virtual(self.s.position);
        let real = self.s.snap_points[self.selected];
        (self.s.snap, self.s.position) = (real, real + (self.s.position - snap.at));
        self.moved = true;
    }

    /// React InterruptSnapping: a snap on its way stops where it is; a looped one is brought, by
    /// whole strips, next to the selected slide.
    fn interrupt_snapping(&mut self) {
        if !self.s.is_animating() {
            return;
        }
        self.s.stop();
        let mut pos = self.s.position;
        if self.looped() && self.selected < self.s.snap_points.len() {
            let s = &self.s.snap_points;
            let strip = (s[1] - s[0]) * s.len() as f32;
            let target = s[self.selected];
            while dist(pos + strip, target) < dist(pos, target) {
                pos += strip;
            }
            while dist(pos - strip, target) < dist(pos, target) {
                pos -= strip;
            }
        }
        (self.s.position, self.s.snap) = (pos, pos);
        self.moved = true;
    }

    // ------------------------------------------------------------ the selected slide

    /// A new index: the app hears of it, a dynamic size is measured again.
    fn report_index(&mut self, index: usize) {
        if index == self.selected {
            return;
        }
        (self.last_index, self.selected, self.p.selected_index) = (Some(self.selected), index, index);
        self.events.push(Event::Index(index));
    }

    /// React UpdateReportedPosition: the index of the anchor the carousel rests on or goes to.
    fn update_reported_position(&mut self) {
        if self.s.snap_points.is_empty() {
            return;
        }
        if self.looped() {
            let index = match self.nearest_virtual(self.s.snap).id {
                -1 => self.max_index(),
                -2 => 0,
                i => i as usize,
            };
            self.report_index(index);
        } else if let Some(i) = self.s.snap_points.iter().position(|p| near(*p, self.s.snap))
            && i < self.count
        {
            self.report_index(i);
        }
    }

    /// React ApplyIndex: to the selected slide; animated, a looped carousel goes from the last to
    /// the first (and back) through the virtual anchor.
    fn apply_index(&mut self, instant: bool) {
        let Some(&real) = self.s.snap_points.get(self.selected) else { return };
        let mut target = real;
        if !instant && self.apply_looped_logic() {
            if self.selected == 0 && self.last_index == Some(self.max_index()) {
                target = self.virtual_anchor(-2);
            } else if self.selected == self.max_index() && self.last_index == Some(0) {
                target = self.virtual_anchor(-1);
            }
        }
        let animate = !instant && self.can_animate() && self.sp.animated;
        self.scroll_to_offset(target, Point::default(), animate);
    }

    /// The React SelectedIndex setter: the app set another slide.
    fn set_selected(&mut self, index: usize) {
        self.interrupt_snapping();
        (self.last_index, self.selected, self.p.selected_index) = (Some(self.selected), index, index);
        self.events.push(Event::Index(index));
        if !self.s.snap_points.is_empty() && !self.s.is_user_panning {
            self.apply_index(false);
        }
    }

    /// React ScrollTo(index, animate).
    fn scroll_to_index(&mut self, index: usize, animate: bool) {
        let index = index.min(self.max_index());
        if index == self.selected {
            self.apply_index(!animate);
        } else if animate {
            self.set_selected(index);
        } else {
            self.interrupt_snapping();
            (self.last_index, self.selected, self.p.selected_index) = (Some(self.selected), index, index);
            self.events.push(Event::Index(index));
            self.apply_index(true);
        }
    }

    // ------------------------------------------------------------ frames

    /// Sets the position and reports what it means (React ApplyPosition).
    fn apply_position(&mut self, position: Point) {
        self.s.position = position;
        self.moved = true;
        self.update_reported_position();
    }

    /// React SnappingLayout.InTransition's setter and SkiaCarousel.OnTransitionChanged.
    fn set_in_transition(&mut self, value: bool) {
        if self.s.in_transition == value {
            return;
        }
        self.s.in_transition = value;
        if !value {
            self.fix_position();
        }
        self.events.push(Event::Transition(value));
    }

    /// React CalculateChildPosition: slide `index` for the current position: its offset from the
    /// start, points; whether it is on screen, next to it; and whether a looped end slide is drawn
    /// on the other side.
    fn child_position(&self, index: usize) -> (f32, bool, bool, bool) {
        let (size, sides, gap) = (if self.p.is_vertical { self.cell.height } else { self.cell.width }, self.p.sides_offset, self.layout.p.spacing);
        let test = |p: f32| (p + sides * 2.0 <= size && p + size >= 0.0, p.abs() - size - sides - gap <= 10.0);
        let pos = self.along(self.s.position) + self.along(self.s.snap_points[index]).abs();
        let (visible, next) = test(pos);
        let count = self.count;
        if self.p.is_looped && count > 1 && (index == 0 || index == count - 1) {
            let step = match self.s.snap_points.len() {
                2.. => (self.along(self.s.snap_points[1]) - self.along(self.s.snap_points[0])).abs(),
                _ => size + gap - sides * 2.0,
            };
            let alt = if index == 0 { pos + step * count as f32 } else { pos - step * count as f32 };
            let (alt_visible, alt_next) = test(alt);
            if (alt_visible || alt_next) && !visible {
                return (alt, alt_visible, alt_next, true);
            }
        }
        (pos, visible, next, false)
    }

    /// Visibility of every slide (ItemAppearing / ItemDisappearing), what is drawn, and which end
    /// slides wrap. True when a wrap changed: the slides are arranged again.
    // ponytail: every slide per moved frame, O(slides); carousels are short. A range from the
    // position would do for long ones.
    fn update_slides(&mut self) -> bool {
        let count = self.count.min(self.s.snap_points.len());
        self.visible.resize(count, false);
        self.shown.resize(count, false);
        let mut wrapped = [false; 2];
        for i in 0..count {
            let (_, on_screen, next, wraps) = self.child_position(i);
            if wraps {
                wrapped[(i != 0) as usize] = true;
            }
            self.shown[i] = on_screen || (next && self.p.preload_neighboors);
            if self.visible[i] != on_screen {
                self.visible[i] = on_screen;
                self.events.push(if on_screen { Event::Appearing(i) } else { Event::Disappearing(i) });
            }
        }
        wrapped != std::mem::replace(&mut self.wrapped, wrapped)
    }

    /// The pixels the slides are drawn moved by.
    fn pixels(&self) -> Point {
        self.s.position * self.s.scale
    }

    fn start_ticking(&mut self, tree: &mut Tree, id: ControlId) {
        if !std::mem::replace(&mut self.ticking, true) {
            animators::start_frame(tree, id, tick);
        }
    }

    // ------------------------------------------------------------ gestures

    /// React resetPan: the finger takes the slides where they are.
    fn reset_pan(&mut self) {
        self.wrong_direction = false;
        (self.s.is_user_focused, self.s.is_user_panning) = (true, false);
        self.snap_if_no_pan_on_up = self.s.is_animating() || self.s.in_transition;
        self.s.stop();
        self.s.accumulator.clear();
        self.fix_position();
        (self.panning_offset, self.panning_start) = (self.s.position, self.s.position);
    }
}

/// Everything a slide needs from the carousel (React AdaptTemplate): it fills the carousel (Start
/// only on an axis the carousel takes from its content and starts at), with `sides_offset` as its
/// margin along the axis. Set without the dirty queue: the slide is measured right after.
fn adapt(tree: &mut Tree, carousel: &SlideFit, child: ControlId) {
    let Some(node) = tree.node_mut(child) else { return };
    let p = &mut node.base.p;
    let options = |start: bool| if start { LayoutOptions::Start } else { LayoutOptions::Fill };
    let (h, v) = (options(carousel.width_start), options(carousel.height_start));
    let m = carousel.sides;
    let margin = if carousel.vertical { Thickness::new(0.0, m, 0.0, m) } else { Thickness::new(m, 0.0, m, 0.0) };
    if p.horizontal_options != h || p.vertical_options != v || p.margin != margin {
        (p.horizontal_options, p.vertical_options, p.margin) = (h, v, margin);
        node.base.need_measure = true;
    }
}

/// What `adapt` reads of the carousel.
struct SlideFit {
    width_start: bool,
    height_start: bool,
    sides: f32,
    vertical: bool,
}

/// Marks the carousel and every control above it for arrange: the slides move to other places.
fn rearrange(tree: &mut Tree, id: ControlId) {
    let mut current = Some(id);
    while let Some(node) = current.and_then(|c| tree.node_mut(c)) {
        node.base.need_arrange = true;
        current = node.parent;
    }
    tree.invalidate(id, Dirty::DRAW);
}

/// One frame: the snap moves, the transition state and the slides follow, then the handlers run.
fn tick(id: ControlId, time_ms: f64, state: &mut dyn Any, cx: &mut Cx<'_>) -> FrameTick {
    let mut tick = FrameTick { keep: false, state_touched: false };
    let Some(mut me) = cx.tree.find_mut::<SkiaCarousel>(id) else { return tick };
    let c = me.control_mut();
    if let Some((position, done)) = c.s.animate(time_ms) {
        c.apply_position(position);
        if done {
            // React OnAnimationStopped.
            let ended = c.s.transition_ended();
            c.set_in_transition(!ended);
            c.update_reported_position();
        }
    }
    // React SnappingLayout.Render: the slides are painted (their visibility), then the transition
    // state follows the position; a looped carousel that came to rest may stand elsewhere then.
    let laid = c.s.snap_points.len() == c.count;
    let mut wraps = laid && c.update_slides();
    let (ended, before) = (c.s.transition_ended(), c.s.position);
    c.set_in_transition(!ended);
    if laid && c.s.position != before {
        wraps |= c.update_slides();
    }
    let moved = std::mem::take(&mut c.moved);
    let (pixels, remeasure) = (c.pixels(), c.p.dynamic_size && c.events.iter().any(|e| matches!(e, Event::Index(_))));
    let mut events = std::mem::take(&mut c.events);
    if moved {
        cx.tree.set_content_offset(id, pixels);
    }
    if wraps {
        rearrange(cx.tree, id);
    }
    if remeasure {
        cx.tree.invalidate(id, Dirty::MEASURE);
    }
    for event in events.drain(..) {
        tick.state_touched |= match event {
            Event::Index(i) => run(cx, id, state, |c| &mut c.on_selected_index_changed, i),
            Event::Appearing(i) => run(cx, id, state, |c| &mut c.on_item_appearing, i),
            Event::Disappearing(i) => run(cx, id, state, |c| &mut c.on_item_disappearing, i),
            Event::Transition(t) => run(cx, id, state, |c| &mut c.on_transition_changed, t),
        };
    }
    let Some(mut me) = cx.tree.find_mut::<SkiaCarousel>(id) else { return tick };
    let c = me.control_mut();
    // The list goes back empty, its capacity kept: a frame allocates nothing.
    if c.events.is_empty() {
        c.events = events;
    }
    c.ticking = c.s.is_animating() || !c.events.is_empty() || c.moved;
    tick.keep = c.ticking;
    tick
}

/// Runs one handler with the carousel as `me`, then puts it back, unless it set another one. True
/// when there was one.
fn run<V>(cx: &mut Cx<'_>, id: ControlId, state: &mut dyn Any, slot: fn(&mut SkiaCarousel) -> &mut Option<Handler<V>>, value: V) -> bool {
    let Some(mut f) = cx.tree.find_mut::<SkiaCarousel>(id).and_then(|mut me| slot(me.control_mut()).take()) else { return false };
    fire(cx, id, |me, cx| f(me, &mut *state, cx, value));
    if let Some(mut me) = cx.tree.find_mut::<SkiaCarousel>(id) {
        let slot = slot(me.control_mut());
        if slot.is_none() {
            *slot = Some(f);
        }
    }
    true
}

impl Has<CarouselProps> for SkiaCarousel {
    fn part(&self) -> &CarouselProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut CarouselProps {
        &mut self.p
    }
}

impl Has<SnappingProps> for SkiaCarousel {
    fn part(&self) -> &SnappingProps {
        &self.sp
    }
    fn part_mut(&mut self) -> &mut SnappingProps {
        &mut self.sp
    }
}

impl Has<LayoutProps> for SkiaCarousel {
    fn part(&self) -> &LayoutProps {
        &self.layout.p
    }
    fn part_mut(&mut self) -> &mut LayoutProps {
        &mut self.layout.p
    }
}

impl Container for SkiaCarousel {}

impl Control for SkiaCarousel {
    fn inner(&self) -> Option<&dyn Control> {
        Some(&self.layout)
    }
    fn inner_mut(&mut self) -> Option<&mut dyn Control> {
        Some(&mut self.layout)
    }

    /// `selected_index` set by the app, or an order from `scroll_to`: the carousel goes there.
    fn on_props_changed(&mut self, cx: &mut Cx) {
        if let Some((index, animate)) = self.order.take() {
            self.scroll_to_index(index, animate);
        } else if self.p.selected_index != self.selected {
            self.set_selected(self.p.selected_index);
        }
        self.p.selected_index = self.selected;
        self.s.started_at(cx.tree.time_ms);
        if let Some(id) = self.id {
            self.start_ticking(cx.tree, id);
        }
    }

    /// Every slide is measured in the carousel's box. The carousel takes the constraints it gets;
    /// on an axis without one, the largest slide, or the selected one with `dynamic_size`.
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        let SkiaLayout { items, p: layout_props, .. } = &mut self.layout;
        let templated = items.is_some();
        // A cell per slide: the carousel places and paints its slides itself, and a shader
        // carousel samples each slide's own cache. ponytail: DrawnUI recycles the views of slides
        // far off screen (ChildrenFactory); do that here when a carousel with many slides needs it.
        if let Some(items) = items.as_deref_mut() {
            items.realize_all(cx, layout_props, false);
        }
        let count = if templated { self.layout.items_count() } else { cx.child_count() };
        let (inner, added) = layout::content_box(&cx.base().p, width, height, cx.scale);
        let bp = &cx.base().p;
        let props = SlideFit {
            width_start: bp.width_request < 0.0 && bp.horizontal_options == LayoutOptions::Start,
            height_start: bp.height_request < 0.0 && bp.vertical_options == LayoutOptions::Start,
            sides: self.p.sides_offset,
            vertical: self.p.is_vertical,
        };
        let selected = self.selected.min(count.saturating_sub(1));
        let mut size = Size::default();
        for i in 0..count {
            let child = cx.child(i);
            if !cx.child_base(child).p.is_visible {
                continue;
            }
            adapt(cx.tree, &props, child);
            let measured = cx.measure_child(child, inner.width, inner.height);
            // Templated slides are one template: the first one sizes the carousel (C#).
            let counts = if self.p.dynamic_size { i == selected } else { !templated || i == 0 };
            if counts {
                size = if self.p.dynamic_size { measured } else { Size::new(size.width.max(measured.width), size.height.max(measured.height)) };
            }
        }
        let side = |constraint: f32, content: f32, added: f32| if constraint.is_finite() { constraint } else { content + added };
        Size::new(side(width, size.width, added.width), side(height, size.height, added.height))
    }

    /// The snap points follow the slide size; every slide goes to its place along the axis.
    fn arrange(&mut self, cx: &mut LayoutCx) {
        let (scale, id) = (cx.scale, cx.id);
        self.id = Some(id);
        let templated = self.layout.items.is_some();
        self.count = if templated { self.layout.items_count() } else { cx.child_count() };
        let inner = layout::content_rect(cx.base(), scale);
        let rect = cx.base().rect;
        (self.s.scale, self.s.size) = (scale, Point::new(rect.width() / scale, rect.height() / scale));
        self.cell = Size::new(inner.width() / scale, inner.height() / scale);
        let along = if self.p.is_vertical { self.cell.height } else { self.cell.width };
        let key = (self.p.is_vertical, self.layout.p.spacing, self.p.sides_offset, along);
        if self.key.is_none() {
            // The index the carousel was built with.
            self.selected = self.p.selected_index;
        }
        if self.key != Some(key) || self.s.snap_points.len() != self.count {
            self.key = Some(key);
            self.initialize_children();
        } else if self.p.is_looped != self.last_looped {
            self.last_looped = self.p.is_looped;
            self.s.bounds = self.bounds();
        }
        self.update_slides();
        let (step, count) = (self.step(), self.count as f32);
        for i in 0..self.count.min(self.s.snap_points.len()) {
            let mut at = self.along(self.s.snap_points[i]).abs();
            if i == 0 && self.wrapped[0] {
                at += step * count;
            } else if i + 1 == self.count && self.wrapped[1] {
                at -= step * count;
            }
            let slot = match self.p.is_vertical {
                true => Rect::from_xywh(inner.left, inner.top + at * scale, inner.width(), inner.height()),
                false => Rect::from_xywh(inner.left + at * scale, inner.top, inner.width(), inner.height()),
            };
            let child = cx.child(i);
            cx.arrange_child(child, slot);
        }
        // The slides are placed for this position already.
        cx.base_mut().content_offset = self.pixels();
        self.moved = false;
        self.start_ticking(cx.tree, id);
    }

    /// The slides on screen, and their neighbors with `preload_neighboors`.
    fn paint(&self, cx: &mut PaintCx) {
        let Some(node) = cx.node(cx.id) else { return };
        for (i, &child) in node.children.iter().enumerate().take(self.shown.len()) {
            if self.shown[i] {
                cx.paint_child(child);
            }
        }
    }

    /// React SkiaCarousel.ProcessGestures: the slides first, while the carousel does not pan; a
    /// press is the carousel's; a pan along its axis moves the slides, a release snaps them.
    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        let (mut passed, mut child) = (false, None);
        if !self.s.is_user_panning || !self.sp.responds_to_gestures || gesture.kind == GestureKind::Tapped {
            (passed, child) = (true, cx.route_children(gesture));
            if let Some(child) = child
                && !(gesture.kind == GestureKind::Up && self.snap_if_no_pan_on_up)
            {
                return Handled::By(child);
            }
        }
        if !self.sp.responds_to_gestures || self.count < 2 {
            return Handled::No;
        }
        let scale = self.s.scale;
        let mut consumed = false;
        match gesture.kind {
            GestureKind::Down => {
                self.had_down = true;
                self.reset_pan();
                consumed = true;
            }
            GestureKind::Panning => {
                if !self.had_down || self.wrong_direction {
                    return Handled::No;
                }
                if !self.s.is_user_panning {
                    let (x, y) = (gesture.total.x.abs(), gesture.total.y.abs());
                    let (along, across) = if self.p.is_vertical { (y, x) } else { (x, y) };
                    // Under 2 points the direction is not known yet: the next move decides (C#
                    // decides on every move). A first move of 1 px on a Retina screen took the
                    // whole drag away from the carousel.
                    if along < scale * 2.0 && across < scale * 2.0 {
                        return Handled::No;
                    }
                    if along < scale * 2.0 || across > along {
                        self.wrong_direction = true;
                        return Handled::No;
                    }
                }
                if !self.s.is_user_focused {
                    self.reset_pan();
                }
                (self.s.is_user_panning, self.snap_if_no_pan_on_up) = (true, false);
                let moved = self.panning_offset + gesture.delta * (1.0 / scale);
                let velocity = gesture.velocity * (1.0 / scale);
                let (velocity, offset) = match self.p.is_vertical {
                    true => (Point::new(0.0, velocity.y), Point::new(0.0, moved.y)),
                    false => (Point::new(velocity.x, 0.0), Point::new(moved.x, 0.0)),
                };
                self.s.accumulator.capture(velocity, gesture.time_ms);
                self.panning_offset = offset;
                let clamped = self.s.clamp(offset.x, offset.y, self.sp.bounces, self.sp.rubber_effect);
                self.apply_position(clamped);
                consumed = true;
            }
            GestureKind::Up => {
                self.had_down = false;
                if self.s.is_user_panning {
                    consumed = true;
                    // React reads the clock while it processes the Up: the frame time here.
                    let velocity = self.s.accumulator.final_velocity(cx.tree.time_ms, 500.0);
                    self.s.snap = self.s.position;
                    self.scroll_to_nearest_anchor(self.s.snap, velocity);
                    (self.s.is_user_panning, self.s.is_user_focused, self.snap_if_no_pan_on_up) = (false, false, false);
                } else if self.snap_if_no_pan_on_up {
                    // A press stopped a snap and no pan followed: it still comes to rest on a slide.
                    self.s.snap = self.s.position;
                    self.scroll_to_nearest_anchor(self.s.snap, Point::default());
                    (self.s.is_user_focused, self.s.is_user_panning, self.snap_if_no_pan_on_up) = (false, false, false);
                }
            }
            _ => {}
        }
        self.start_ticking(cx.tree, cx.id);
        if consumed {
            return Handled::Yes;
        }
        if let Some(child) = child {
            return Handled::By(child);
        }
        if self.s.is_user_panning {
            return if gesture.kind == GestureKind::Up { Handled::No } else { Handled::Yes };
        }
        if !passed {
            return cx.route_children(gesture).map_or(Handled::No, Handled::By);
        }
        Handled::No
    }
}

impl SkiaCarousel {
    /// React InitializeChildren: snap points from the slide size, bounds, the selected slide at once.
    fn initialize_children(&mut self) {
        self.last_looped = self.p.is_looped;
        let (step, vertical) = (self.step(), self.p.is_vertical);
        self.s.snap_points = (0..self.count).map(|i| if vertical { Point::new(0.0, -(i as f32) * step) } else { Point::new(-(i as f32) * step, 0.0) }).collect();
        self.s.bounds = self.bounds();
        self.s.snap = Point::new(-1.0, -1.0);
        self.visible.clear();
        if self.count > 0 && self.selected > self.max_index() {
            (self.last_index, self.selected, self.p.selected_index) = (Some(self.selected), 0, 0);
            self.events.push(Event::Index(0));
        }
        self.apply_index(true);
    }

    /// React GetContentOffsetBounds: the extent of the snap points, endless along the axis when looped.
    fn bounds(&self) -> Rect {
        let b = self.s.bounds_from_snap_points();
        if !self.p.is_looped || self.s.snap_points.len() < 2 {
            return b;
        }
        let far = 1e9;
        if self.p.is_vertical { Rect::new(b.left, -far, b.right, far) } else { Rect::new(-far, b.top, far, b.bottom) }
    }
}

fn carousel_part<T: Control>(control: &mut T) -> &mut SkiaCarousel {
    part_mut(control).expect("the control embeds a SkiaCarousel")
}

fn index_handler<T: Control, S: Any>(mut f: impl FnMut(&mut Mut<'_, T>, &mut S, &mut Cx<'_>, usize) + 'static) -> IndexHandler {
    Box::new(move |me, state, cx, index| {
        let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
        f(&mut me.typed(), state, cx, index)
    })
}

impl<T: Has<CarouselProps>> Build<T> {
    /// Runs when the selected slide changes, by a swipe or by code (React SelectedIndexChanged).
    pub fn on_selected_index_changed<S: Any>(mut self, f: impl FnMut(&mut Mut<'_, T>, &mut S, &mut Cx<'_>, usize) + 'static) -> Self {
        carousel_part(self.control_mut()).on_selected_index_changed = Some(index_handler(f));
        self
    }

    /// Runs when the slides start moving between two anchors (true) and when they came to rest on
    /// one (false) (React TransitionChanged).
    pub fn on_transition_changed<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, T>, &mut S, &mut Cx<'_>, bool) + 'static) -> Self {
        carousel_part(self.control_mut()).on_transition_changed = Some(Box::new(move |me, state, cx, value| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
            f(&mut me.typed(), state, cx, value)
        }));
        self
    }

    /// Runs when a slide comes on screen (React ItemAppearing).
    pub fn on_item_appearing<S: Any>(mut self, f: impl FnMut(&mut Mut<'_, T>, &mut S, &mut Cx<'_>, usize) + 'static) -> Self {
        carousel_part(self.control_mut()).on_item_appearing = Some(index_handler(f));
        self
    }

    /// Runs when a slide leaves the screen (React ItemDisappearing).
    pub fn on_item_disappearing<S: Any>(mut self, f: impl FnMut(&mut Mut<'_, T>, &mut S, &mut Cx<'_>, usize) + 'static) -> Self {
        carousel_part(self.control_mut()).on_item_disappearing = Some(index_handler(f));
        self
    }
}

impl Mut<'_, SkiaCarousel> {
    /// The next slide; from the last one to the first when looped (React GoNext).
    pub fn go_next(&mut self) {
        let (selected, max, looped) = (self.selected, self.max_index(), self.p.is_looped);
        if selected < max {
            self.set_selected_index(selected + 1);
        } else if looped {
            self.set_selected_index(0);
        }
    }

    /// The slide before; from the first one to the last when looped (React GoPrev).
    pub fn go_prev(&mut self) {
        let (selected, max, looped) = (self.selected, self.max_index(), self.p.is_looped);
        if selected > 0 {
            self.set_selected_index(selected - 1);
        } else if looped {
            self.set_selected_index(max);
        }
    }

    /// Goes to slide `index` (kept within the slides), animated or at once (React ScrollTo).
    pub fn scroll_to(&mut self, index: usize, animate: bool) {
        self.control_mut().order = Some((index, animate));
        self.mark(Dirty::APPLY);
    }
}
