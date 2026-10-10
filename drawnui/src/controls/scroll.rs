//! SkiaScroll: a viewport over one content child, moved by a pan, a fling, the wheel or code.
//! It scrolls by setting its own `content_offset`: no layout runs for a scroll frame. Offsets are
//! points and negative while scrolled (the content moves up / left), as in DrawnUI.
//!
//! One frame animator per scrolling SkiaScroll gives it the frame time; the physics (DrawnUI
//! ScrollFlingAnimator, SpringWithVelocityAnimator, RubberBandUtils) live here. Where the C#
//! engine and the React engine differ, the React rules are the ones ported.

use std::any::Any;

use skia_safe::{ClipOp, Color, Contains, Point, Rect, Size};

use crate::animators::{self, FrameTick, easing};
use crate::control::{Control, GestureCx, Handled, Has, LayoutCx, PaintCx, part_mut};
use crate::controls::layout::SkiaLayout;
use crate::controls::scroll_bar::{self, ScrollBarSet, SkiaScrollBar};
use crate::gestures::{Gesture, GestureKind, VelocityAccumulator};
use crate::{layout, paint, props};
use crate::tree::{Build, ControlId, Cx, Detached, Mut, Raw, Tree, wrong_state};
use crate::types::IntoProp;

/// Points one wheel notch scrolls.
pub const WHEEL_LINE_SIZE: f32 = 150.0;
/// A pan slower than this along the scroll axis (points per second) does not start scrolling.
const SCROLL_VELOCITY_THRESHOLD: f32 = 5.0;
/// A release slower than this does not fling. Points per second times the scale, as upstream.
const THRESHOLD_SWIPE_ON_UP: f32 = 20.0;
const MIN_VELOCITY: f32 = 1.5;
/// The share of each move the content follows at once; the rest is carried into the next move.
const PAN_SMOOTHING: f32 = 0.85;
/// The rubber band of a viewport without a size (React RubberBandClamp onEmpty).
const RUBBER_ON_EMPTY: f32 = 40.0;
/// A fling slower than this (points per second) for three frames in a row is over.
const FLING_VALUE_THRESHOLD: f64 = 1.85;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ScrollOrientation {
    #[default]
    Vertical,
    Horizontal,
    Both,
    Neither,
}

/// Where a row lands in the viewport for `scroll_to_index`, and the point of the viewport
/// `track_index_position` looks at.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RelativePositionType {
    /// As Start for `scroll_to_index`; no tracking.
    None,
    #[default]
    Start,
    Center,
    End,
}

/// The scroll bars a scroll makes for itself (DrawnUI ScrollBarsVisibility).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ScrollBarVisibility {
    #[default]
    None,
    Vertical,
    Horizontal,
    Both,
}

/// Where a scroll comes to rest after a pan, a fling or a bounce (DrawnUI SnapToChildrenType).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SnapToChildrenType {
    #[default]
    Disabled,
    /// The start of the tracked child (`track_index_position` Start) or its end (End) goes to
    /// the tracked point.
    Side,
    /// The center of the tracked child goes to the tracked point.
    Center,
}

impl ScrollOrientation {
    fn along_x(self) -> bool {
        matches!(self, Self::Horizontal | Self::Both)
    }
    fn along_y(self) -> bool {
        matches!(self, Self::Vertical | Self::Both)
    }
}

props!(ScrollProps, ScrollBuild, ScrollSet {
    orientation / set_orientation: ScrollOrientation = ScrollOrientation::Vertical, MEASURE;
    /// The content can be pulled past its edges and springs back.
    bounces / set_bounces: bool = true, NONE;
    /// How fast a fling slows down: 0.1 and up, lower = longer.
    friction_scrolled / set_friction_scrolled: f32 = 0.3, NONE;
    /// Multiplies the release velocity of a fling.
    change_velocity_scrolled / set_change_velocity_scrolled: f32 = 1.33, NONE;
    /// Multiplies the distance a pan moves the content.
    change_distance_panned / set_change_distance_panned: f32 = 1.0, NONE;
    /// Limit of the release velocity, points per second.
    max_velocity / set_max_velocity: f32 = 3000.0, NONE;
    /// Limit of the velocity a bounce starts with, points per second.
    max_bounce_velocity / set_max_bounce_velocity: f32 = 500.0, NONE;
    /// How far the content can be pulled past an edge: higher = further. It stretches over the
    /// viewport at most (React), where C# uses 100 points times the scale.
    rubber_effect / set_rubber_effect: f32 = 0.55, NONE;
    /// The spring of the bounce back is built from it: mass 1 + it, damping ratio (1 + it) / 2.
    rubber_damping / set_rubber_damping: f32 = 0.55, NONE;
    /// Milliseconds of a wheel step.
    scrolling_speed_ms / set_scrolling_speed_ms: f32 = 400.0, NONE;
    /// Milliseconds of a snap and of the way back after a refresh.
    auto_scrolling_speed_ms / set_auto_scrolling_speed_ms: f32 = 600.0, NONE;
    /// A pan that moves more across the scroll axis than along it does not scroll.
    ignore_wrong_direction / set_ignore_wrong_direction: bool = false, NONE;
    /// Off: pans and the wheel do not scroll, code still does.
    responds_to_gestures / set_responds_to_gestures: bool = true, NONE;
    /// Points from the end of the content where the load-more handler fires.
    load_more_offset / set_load_more_offset: f32 = 0.0, NONE;
    /// Points from the start of the content where the load-more-top handler fires.
    load_more_top_offset / set_load_more_top_offset: f32 = 0.0, NONE;
    /// The header stays at the start of the viewport while the content scrolls under it.
    header_sticky / set_header_sticky: bool = false, MEASURE;
    /// The header is drawn under the content, which covers it as it scrolls.
    header_behind / set_header_behind: bool = false, MEASURE;
    /// How much of the scrolling a header that is not sticky follows: 1 = goes with the content,
    /// 0.5 = at half the speed.
    header_parallax_ratio / set_header_parallax_ratio: f32 = 1.0, MEASURE;
    /// Off: while the content is pulled past its start the header goes with it, without parallax.
    parallax_overscroll_enabled / set_parallax_overscroll_enabled: bool = true, MEASURE;
    /// Points added between the header and the content; negative lets the content cover a
    /// `header_behind` header. As upstream: it moves the content only under a sticky or behind
    /// header, but always counts for the scrolled length and for where the footer stands.
    content_offset / set_content_offset: f32 = 0.0, MEASURE;
    /// Thumb and track color given to the scroll bars; `None` leaves them their own.
    scroll_bar_thumb_color / set_scroll_bar_thumb_color: Option<Color> = None, APPLY;
    scroll_bar_track_color / set_scroll_bar_track_color: Option<Color> = None, APPLY;
    /// Scroll bars that hide by themselves stay visible while this is set.
    keep_scroll_bars_visible / set_keep_scroll_bars_visible: bool = false, APPLY;
    /// The point of the viewport that says which child of the content is the current one: the
    /// start of the viewport, its center, or `track_index_position_offset` before its end.
    track_index_position / set_track_index_position: RelativePositionType = RelativePositionType::None, APPLY;
    /// Points into the viewport that `Start` looks at, and before its end for `End`.
    track_index_position_offset / set_track_index_position_offset: f32 = 8.0, APPLY;
    /// When a pan, a fling, a bounce, a wheel step or an animated `scroll_to` comes to rest, the
    /// viewport goes on until the child under the snap point stands on it (React: once per press):
    /// its middle on the middle for Center, its start or end on the tracked point for Side.
    snap_to_children / set_snap_to_children: SnapToChildrenType = SnapToChildrenType::Disabled, NONE;
    /// A pull past the start shows the `refresh_indicator`; held further than
    /// `refresh_distance_limit` it starts a refresh. Vertical scrolls only.
    refresh_enabled / set_refresh_enabled: bool = false, NONE;
    /// A pull that starts a refresh sets it and runs `on_refresh`; released, the content comes
    /// back to `refresh_show_distance` and stays there until the app sets it back (a pan still
    /// moves it, as React). Set by the app, it starts a refresh too.
    is_refreshing / set_is_refreshing: bool = false, APPLY;
    /// Points the content must be pulled for a refresh, `refresh_show_distance` at least.
    refresh_distance_limit / set_refresh_distance_limit: f32 = 150.0, NONE;
    /// Points of pull at which the indicator is all there.
    refresh_show_distance / set_refresh_show_distance: f32 = 50.0, NONE;
});

// ---------------------------------------------------------------- physics

/// Exponential deceleration (DrawnUI DecelerationTimingParameters):
/// value(t) = value + velocity * (rate^(1000 t) - 1) / k, with k = 1000 ln(rate). In f64, as
/// React's numbers: k is the log of a rate a hair below 1, which f32 gets 8e-6 wrong, 0.01 point
/// over a long fling.
#[derive(Clone, Copy)]
struct Deceleration {
    value: f64,
    velocity: f64,
    rate: f64,
    k: f64,
}

impl Deceleration {
    /// The speed (per second) at which the curve counts as stopped.
    const THRESHOLD: f64 = 0.001;

    fn new(value: f32, velocity: f32, rate: f64) -> Self {
        Self { value: value as f64, velocity: velocity as f64, rate, k: 1000.0 * rate.ln() }
    }
    /// Where the curve ends.
    fn destination(&self) -> f32 {
        (if self.velocity == 0.0 { self.value } else { self.value - self.velocity / self.k }) as f32
    }
    fn duration_secs(&self) -> f32 {
        if self.velocity == 0.0 {
            return 0.0;
        }
        let divisor = -self.k * Self::THRESHOLD / self.velocity.abs();
        (if divisor <= 0.0 { 0.0 } else { divisor.ln() / self.k }) as f32
    }
    fn decay(&self, secs: f32) -> f64 {
        self.rate.powf(1000.0 * secs as f64)
    }
    fn value_at(&self, secs: f32) -> f32 {
        self.value_at64(secs) as f32
    }
    fn value_at64(&self, secs: f32) -> f64 {
        self.value + self.velocity * (self.decay(secs) - 1.0) / self.k
    }
    fn velocity_at(&self, secs: f32) -> f32 {
        (self.velocity * self.decay(secs)) as f32
    }
    /// Seconds until the curve reaches `value`.
    fn duration_to_value(&self, value: f32) -> f32 {
        let distance = (value as f64 - self.value).abs();
        if self.velocity == 0.0 || distance == 0.0 {
            return 0.0;
        }
        ((1.0 + self.k * distance / self.velocity.abs()).ln() / self.k) as f32
    }
}

/// The deceleration rate of a scroll's friction (React 1 - DecelerationRatio).
fn rate(friction: f32) -> f64 {
    1.0 - friction.max(0.1) as f64 / 100.0
}

/// React / DrawnUI ScrollFlingAnimator. React has no pixel-aware finish (C# eases the tail onto a
/// whole pixel below scale 1.5): the curve runs out as it is.
#[derive(Clone, Copy)]
struct Fling {
    curve: Deceleration,
    /// Seconds the fling runs (upstream Speed): the whole curve, or up to `edge`.
    secs: f32,
    /// The content edge the curve would pass: the fling is cut to stop there.
    edge: Option<f32>,
    /// Points per second at the last frame of the curve.
    velocity: f32,
    last_value: f64,
    last_secs: f32,
    slow_frames: u8,
}

impl Fling {
    /// The value at `secs` from the start, and whether the fling is over. `dt` = seconds since the last frame.
    fn update(&mut self, dt: f32, secs: f32) -> (f32, bool) {
        // Past its time it lands where the planned time ends: the edge, or the end of the curve.
        if secs > self.secs {
            self.velocity = self.curve.velocity_at(self.secs);
            return (self.curve.value_at(self.secs), true);
        }
        let value = self.curve.value_at64(secs);
        self.velocity = self.curve.velocity_at(secs);
        // Not before the third frame, as upstream.
        if self.last_secs > 0.0 {
            let rate = if dt > 0.0 { (value - self.last_value).abs() / dt as f64 } else { 0.0 };
            if rate < FLING_VALUE_THRESHOLD {
                self.slow_frames += 1;
                if self.slow_frames >= 3 {
                    return (value as f32, true);
                }
            } else {
                self.slow_frames = 0;
            }
        }
        (self.last_value, self.last_secs) = (value, secs);
        (value as f32, false)
    }
}

/// The way back from an overscroll (DrawnUI SpringWithVelocityAnimator, underdamped spring):
/// origin + e^(-beta t) * (c1 cos(wd t) + c2 sin(wd t)).
#[derive(Clone, Copy)]
pub(crate) struct Bounce {
    origin: f32,
    beta: f32,
    wd: f32,
    c1: f32,
    c2: f32,
    secs: f32,
}

impl Bounce {
    /// ponytail: the underdamped spring only; a `rubber_damping` of 1 and more is taken just below it.
    fn new(origin: f32, displacement: f32, velocity: f32, damping: f32) -> Self {
        Self::with_spring(origin, displacement, velocity, 1.0 + damping, 200.0, (0.5 * (1.0 + damping)).min(0.999))
    }

    /// The spring of DrawnUI `Spring(mass, stiffness, damping ratio)`, from `displacement` and
    /// `velocity` back to `origin`.
    pub(crate) fn with_spring(origin: f32, displacement: f32, velocity: f32, mass: f32, stiffness: f32, damping_ratio: f32) -> Self {
        let w0 = (stiffness / mass).sqrt();
        let beta = damping_ratio * w0 * 2.0;
        let wd = w0 * (1.0 - damping_ratio * damping_ratio).sqrt();
        let c2 = (velocity + beta * displacement) / wd;
        // Until the amplitude is under half a point.
        let secs = ((displacement.abs() + c2.abs()) / 0.5).ln() / beta;
        Self { origin, beta, wd, c1: displacement, c2, secs }
    }

    pub(crate) fn update(&self, secs: f32) -> (f32, bool) {
        if secs > self.secs {
            return (self.origin, true);
        }
        let swing = self.c1 * (self.wd * secs).cos() + self.c2 * (self.wd * secs).sin();
        (self.origin + (-self.beta * secs).exp() * swing, false)
    }
}

/// An animated `scroll_to`, a wheel step or a snap: the deceleration curve with the velocity that
/// lands on `to` after `ms` (React `ScrollFlingAnimator.InitializeWithDestination`, DrawnUI
/// DecelerationTimingParameters' second constructor). It ends there, or as soon as it moves
/// slower than 0.1 points per second for three frames; either way it lands on `to` exactly.
#[derive(Clone, Copy)]
struct Range {
    curve: Deceleration,
    to: f32,
    secs: f32,
    last_value: f64,
    last_secs: f32,
    slow_frames: u8,
}

impl Range {
    const VALUE_THRESHOLD: f64 = 0.1;

    fn new(from: f32, to: f32, ms: f32, rate: f64) -> Self {
        let mut curve = Deceleration::new(from, 0.0, rate);
        let (distance, secs) = ((to - from) as f64, ms as f64 / 1000.0);
        let denominator = rate.powf(1000.0 * secs) - 1.0;
        curve.velocity = if denominator.abs() < 1e-5 { distance / secs } else { distance * curve.k / denominator };
        Self { curve, to, secs: secs as f32, last_value: from as f64, last_secs: 0.0, slow_frames: 0 }
    }

    fn update(&mut self, dt: f32, secs: f32) -> (f32, bool) {
        if secs > self.secs {
            return (self.to, true);
        }
        let value = self.curve.value_at64(secs);
        if self.last_secs > 0.0 {
            let rate = if dt > 0.0 { (value - self.last_value).abs() / dt as f64 } else { 0.0 };
            if rate < Self::VALUE_THRESHOLD {
                self.slow_frames += 1;
                if self.slow_frames >= 3 {
                    return (self.to, true);
                }
            } else {
                self.slow_frames = 0;
            }
        }
        (self.last_value, self.last_secs) = (value, secs);
        (value as f32, false)
    }
}

/// A wheel step (C# RangeAnimator with Easing.SpringOut, the overshoot cut off at the destination).
/// React's wheel is not ported: it counts only the sign of a delta (a touchpad share scrolls a
/// whole step), adds a step to the end of the running curve instead of its target, and so runs away.
#[derive(Clone, Copy)]
struct Step {
    from: f32,
    to: f32,
    ms: f32,
}

impl Step {
    fn update(&self, secs: f32) -> (f32, bool) {
        let progress = secs * 1000.0 / self.ms;
        let value = self.from + easing::spring_out(progress) * (self.to - self.from);
        (value.clamp(self.from.min(self.to), self.from.max(self.to)), progress >= 1.0)
    }
}

#[derive(Clone, Copy)]
enum Motion {
    Fling(Fling),
    Bounce(Bounce),
    Range(Range),
    Step(Step),
}

/// What moves one axis by itself, if anything.
#[derive(Default)]
struct Axis {
    motion: Option<Motion>,
    /// Frame time of the first frame of the motion.
    start_ms: Option<f64>,
    /// Seconds from the start at the previous frame.
    last_secs: f32,
}

impl Axis {
    fn start(&mut self, motion: Motion) {
        *self = Axis { motion: Some(motion), start_ms: None, last_secs: 0.0 };
    }
}

// ---------------------------------------------------------------- control

type Scrolled = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>, Point)>;
type LoadMore = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>)>;
type IndexChanged = Box<dyn FnMut(Raw<'_>, &mut dyn Any, &mut Cx<'_>, Option<usize>)>;

/// Which child is what, by the order they were given in: the ids are not known while the scroll
/// is built.
#[derive(Clone, Copy, Default)]
struct Slots {
    count: usize,
    content: Option<usize>,
    header: Option<usize>,
    footer: Option<usize>,
    indicator: Option<usize>,
}

fn slot(tree: &Tree, id: ControlId, at: Option<usize>) -> Option<ControlId> {
    tree.children(id).get(at?).copied()
}

/// What the tree must hold for an offset: the pixels the children are moved by, the paint
/// offset (points) of a header that does not simply go with them, and where the refresh
/// indicator stands (points from the start of the viewport) and how visible it is.
#[derive(Clone, Copy)]
struct Placement {
    pixels: Point,
    header: Option<usize>,
    header_shift: Point,
    indicator: Option<usize>,
    indicator_top: f32,
    indicator_opacity: f32,
}

impl Placement {
    /// Marks what changed: the frame that follows paints it.
    fn write(self, tree: &mut Tree, id: ControlId) {
        tree.set_content_offset(id, self.pixels);
        if let Some(mut header) = slot(tree, id, self.header).and_then(|header| tree.any_mut(header)) {
            header.set_left(self.header_shift.x);
            header.set_top(self.header_shift.y);
        }
        if let Some(mut indicator) = slot(tree, id, self.indicator).and_then(|indicator| tree.any_mut(indicator)) {
            indicator.set_translation_y(self.indicator_top);
            indicator.set_opacity(self.indicator_opacity);
        }
    }
}

/// What a scroll bar indicates (the arguments of DrawnUI IScrollBar.SetScrollProgress).
pub(crate) struct BarView {
    pub horizontal: bool,
    /// 0 at the start, 1 at the end; outside of that while the content is pulled past an edge.
    pub progress: f32,
    /// The viewport as a share of what scrolls; 1 and more: the content fits.
    pub ratio: f32,
    /// Points past an edge.
    pub overscroll: f32,
}

/// A `scroll_to_index` that has not arrived yet (DrawnUI OrderedScrollToIndex).
#[derive(Clone, Copy)]
struct Order {
    index: usize,
    position: RelativePositionType,
    /// Duration of the first move. What follows are corrections, and they jump.
    ms: f32,
    /// Frames the order was open without the viewport moving.
    stalled: u8,
}

#[derive(Default)]
pub struct SkiaScroll {
    pub p: ScrollProps,
    /// Known after the first arrange.
    id: Option<ControlId>,
    /// Points; 0 at the start, negative when scrolled (DrawnUI ViewportOffsetX / ViewportOffsetY).
    offset: Point,
    /// The most negative offset per axis, points (DrawnUI ContentOffsetBounds); the other end is 0.
    min: Point,
    slots: Slots,
    /// The vertical and the horizontal scroll bar (DrawnUI ScrollBar, ScrollBarHorizontal).
    bars: [Option<ControlId>; 2],
    /// What the bars were last stepped for: offset, scrolled length, viewport, scrolling.
    bars_seen: Option<(Point, Size, Size, bool)>,
    /// The bar that is dragged (true = the horizontal one) and where its thumb is held, pixels.
    drag: Option<(bool, f32)>,
    /// The press started on a bar: the tap that ends it is the bar's too.
    bar_owns_gesture: bool,
    /// The measured content, what scrolls (the content plus header and footer) and the viewport
    /// at the last arrange, pixels.
    content: Size,
    extent: Size,
    viewport: Size,
    /// Where the viewport starts, pixels.
    viewport_origin: Point,
    scale: f32,
    x: Axis,
    y: Axis,
    /// Frame time of the last tick: a motion started during layout counts from there.
    tick_ms: f64,
    had_down: bool,
    /// A pointer is down and its pan baseline is set.
    is_user_focused: bool,
    is_user_panning: bool,
    /// Velocity of the last pan move, points per second.
    velocity: Point,
    accumulator: VelocityAccumulator,
    panning_last_delta: Point,
    /// Where the pan would be without the clamp at the edges, points.
    panning_offset: Point,
    order: Option<Order>,
    /// The child of the content at the tracked point (DrawnUI CurrentIndexHit): its index, and
    /// its start and end along the scroll axis, pixels, as `tracked` sees them.
    current: Option<(usize, f32, f32)>,
    /// The offset `current` was looked up for; `None`: it must be looked up again.
    index_at: Option<Point>,
    /// Something moved the content: when all of it comes to rest, a snap is looked at.
    snap_due: bool,
    /// One snap per press (React `snapped`): set by a snap, cleared by the next press.
    snapped: bool,
    /// `is_refreshing` as the scroll has acted on it.
    refreshing: bool,
    /// A refresh started and the content did not go back since: a pull starts no other one.
    was_refreshing: bool,
    /// `on_refresh` must run.
    refresh_due: bool,
    /// Height of the refresh indicator, points.
    indicator_height: f32,
    /// The frame animator of this scroll is registered.
    ticking: bool,
    /// A pan or an animation moved the offset since the last tick: `on_scrolled` is due.
    scrolled: bool,
    /// The offset or the bounds changed since the edges were last looked at.
    check_edges: bool,
    /// The content at the last call at the end (the scrolled length in points, or the item count of
    /// a list) and its frame time. It calls again once the content changed, or the user went away
    /// from the end (React CheckLoadMore).
    load_more_at: Option<(f32, f64)>,
    /// The start calls once the user was 100 points away from it and comes back (React).
    load_more_top_armed: bool,
    /// A child took the pan of this press: the scroll does not pan in it (React childWasPanning).
    child_was_panning: bool,
    on_scrolled: Option<Scrolled>,
    on_load_more: Option<LoadMore>,
    on_load_more_top: Option<LoadMore>,
    on_index_changed: Option<IndexChanged>,
    on_refresh: Option<LoadMore>,
}

impl SkiaScroll {
    /// A vertical scroll without content; give it one with `content`.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Build<SkiaScroll> {
        Build::new(SkiaScroll::default())
    }

    /// Points; 0 at the start, negative when scrolled.
    pub fn viewport_offset_x(&self) -> f32 {
        self.offset.x
    }
    pub fn viewport_offset_y(&self) -> f32 {
        self.offset.y
    }
    /// The axes the content can move along now (horizontal, vertical): the orientation allows it
    /// and the content is larger than the viewport.
    pub fn scroll_axes(&self) -> (bool, bool) {
        let (h, v) = match self.p.orientation {
            ScrollOrientation::Vertical => (false, true),
            ScrollOrientation::Horizontal => (true, false),
            ScrollOrientation::Both => (true, true),
            ScrollOrientation::Neither => (false, false),
        };
        (h && self.min.x < 0.0, v && self.min.y < 0.0)
    }
    /// Measured size of the content, pixels.
    pub fn content_size(&self) -> Size {
        self.content
    }
    pub fn is_user_panning(&self) -> bool {
        self.is_user_panning
    }
    /// A fling, a bounce, a wheel step or an animated `scroll_to` is running.
    pub fn is_animating(&self) -> bool {
        self.x.motion.is_some() || self.y.motion.is_some()
    }
    /// Points the offset is past an edge: positive past the start, negative past the end.
    pub fn overscroll_distance(&self) -> Point {
        self.offset - self.inside(self.offset)
    }
    /// A `scroll_to_index` is still on its way: the row is not measured or not reached yet.
    pub fn has_pending_scroll_order(&self) -> bool {
        self.order.is_some()
    }

    /// The child of the content at the tracked point, counted among its visible children (DrawnUI
    /// CurrentIndex). `None` without `track_index_position`, and while the point is between two
    /// children.
    pub fn current_index(&self) -> Option<usize> {
        self.current.map(|(index, ..)| index)
    }

    /// A point of the viewport, pixels from its start along the scroll axis (React GetIndexHit):
    /// `track_index_position_offset` into it, its middle, or that much before its end.
    fn viewport_point(&self, position: RelativePositionType) -> Option<f32> {
        let viewport = match self.p.orientation {
            ScrollOrientation::Vertical => self.viewport.height,
            ScrollOrientation::Horizontal => self.viewport.width,
            _ => return None,
        };
        let offset = self.p.track_index_position_offset * self.scale;
        match position {
            RelativePositionType::None => None,
            RelativePositionType::Start => Some(offset),
            RelativePositionType::Center => Some(viewport / 2.0),
            RelativePositionType::End => Some(viewport - offset),
        }
    }

    /// The point a snap looks at (React Snap): the middle for Center, else the side
    /// `track_index_position` names, the start unless it is End.
    fn snap_position(&self) -> RelativePositionType {
        match (self.p.snap_to_children, self.p.track_index_position) {
            (SnapToChildrenType::Disabled, _) => RelativePositionType::None,
            (SnapToChildrenType::Center, _) => RelativePositionType::Center,
            (_, RelativePositionType::End) => RelativePositionType::End,
            _ => RelativePositionType::Start,
        }
    }

    /// React CheckNeedToSnap: a snap is looked at when a pan, a fling, a bounce, a wheel step
    /// or a `scroll_to` came to rest, unless one snapped already since the last press, or the
    /// pointer is down.
    fn may_snap(&self) -> bool {
        self.p.snap_to_children != SnapToChildrenType::Disabled && !self.snapped && !self.is_user_focused && !self.is_animating()
    }

    /// React Snap: the viewport goes on, over `auto_scrolling_speed_ms`, until the child under the
    /// snap point stands on it with its middle, its start or its end. `hit` is that child: its
    /// start and end along the axis, layout pixels. Not for less than 2 points.
    fn snap(&mut self, hit: Option<(usize, f32, f32)>) {
        let (Some((_, start, end)), Some(point)) = (hit, self.viewport_point(self.snap_position())) else { return };
        let anchor = match self.snap_position() {
            RelativePositionType::Center => (start + end) / 2.0,
            RelativePositionType::End => end,
            _ => start,
        };
        let (horizontal, mut to) = (self.horizontal(), self.offset);
        let viewport_start = if horizontal { self.viewport_origin.x } else { self.viewport_origin.y };
        let target = (point - (anchor - viewport_start)) / self.scale;
        if (target - *self.along(horizontal)).abs() <= 2.0 {
            return;
        }
        *(if horizontal { &mut to.x } else { &mut to.y }) = target;
        self.snapped = true;
        self.scroll_to(to, self.p.auto_scrolling_speed_ms);
    }

    /// The nearest offset inside the content.
    fn inside(&self, offset: Point) -> Point {
        Point::new(offset.x.clamp(self.min.x, 0.0), offset.y.clamp(self.min.y, 0.0))
    }

    fn axis(&mut self, horizontal: bool) -> &mut Axis {
        if horizontal { &mut self.x } else { &mut self.y }
    }

    fn along(&mut self, horizontal: bool) -> &mut f32 {
        if horizontal { &mut self.offset.x } else { &mut self.offset.y }
    }

    /// What `content_offset` must be: whole pixels, moving or not (React; C# rounds at rest only),
    /// so that text keeps its sub-pixel phase from frame to frame while it scrolls.
    fn pixel_offset(&self) -> Point {
        let p = self.offset * self.scale;
        // JavaScript's Math.round: a half goes up.
        Point::new((p.x + 0.5).floor(), (p.y + 0.5).floor())
    }

    /// Header and footer sit on the horizontal axis only in a horizontal scroll.
    fn horizontal(&self) -> bool {
        self.p.orientation == ScrollOrientation::Horizontal
    }

    fn placement(&self) -> Placement {
        let pixels = self.pixel_offset();
        // All children are drawn moved by `pixels`. A sticky header takes all of it back, a
        // parallax header the share it does not follow (DrawnUI ParallaxComputedValue).
        let follows = if self.p.header_sticky { 0.0 } else { self.p.header_parallax_ratio };
        let over = self.overscroll_distance();
        let pulled = !self.p.header_sticky && !self.p.parallax_overscroll_enabled && (over.x > 0.0 || over.y > 0.0);
        let back = if pulled || self.scale == 0.0 { 0.0 } else { (follows - 1.0) / self.scale };
        let header_shift = if self.horizontal() { Point::new(pixels.x * back, 0.0) } else { Point::new(0.0, pixels.y * back) };
        let (indicator_top, indicator_opacity) = self.indicator();
        Placement { pixels, header: self.slots.header, header_shift, indicator: self.slots.indicator, indicator_top, indicator_opacity }
    }

    /// React RefreshIndicator.SetDragRatio: where the indicator stands in the viewport (points) and
    /// how visible it is. Its far edge follows the pull up to `refresh_show_distance`, where it
    /// stays, centered in the gap when that is taller than it; the opacity is the pull as a share
    /// of that distance. While refreshing it is all there. (C# has another position curve.)
    fn indicator(&self) -> (f32, f32) {
        let (over, show, height) = (self.overscroll_distance().y, self.p.refresh_show_distance, self.indicator_height);
        let ratio = if self.refreshing { 1.0 } else if show > 0.0 { (over / show).clamp(0.0, 1.0) } else { 0.0 };
        if self.slots.indicator.is_none() || !self.p.refresh_enabled || ratio <= 0.0 {
            return (-height, 0.0);
        }
        let shown = if self.refreshing { show } else { self.offset.y.max(0.0).min(show) };
        (shown - height + ((show - height) / 2.0).max(0.0), ratio)
    }

    /// DrawnUI SetIsRefreshing: `is_refreshing` was set, by a pull or by the app. A refresh holds
    /// the content pulled by `refresh_show_distance` at least and runs `on_refresh`; when it is
    /// over the content goes back to its start.
    fn sync_refresh(&mut self, time_ms: f64) {
        if self.p.is_refreshing == self.refreshing {
            return;
        }
        self.refreshing = self.p.is_refreshing;
        self.stop_scrolling();
        if self.refreshing {
            (self.was_refreshing, self.refresh_due, self.order) = (true, true, None);
            if self.overscroll_distance().y <= self.p.refresh_show_distance {
                self.offset.y = self.p.refresh_show_distance;
            }
        } else if !self.offset.is_zero() {
            self.was_refreshing = false;
            self.scroll_to(Point::default(), self.p.auto_scrolling_speed_ms);
            // The app set it between frames: React starts the way back at once, its first frame is
            // the next one at 0 s. Taken during that frame here: the way back counts from it.
            (self.x.start_ms, self.y.start_ms) = (Some(time_ms), Some(time_ms));
        }
        self.check_edges = true;
    }

    /// For a bar of this scroll: what it shows now.
    pub(crate) fn bar_view(&self, bar: ControlId) -> Option<BarView> {
        let horizontal = self.bars[1] == Some(bar);
        if !horizontal && self.bars[0] != Some(bar) {
            return None;
        }
        let (offset, min, over) = (self.offset, self.min, self.overscroll_distance());
        let (offset, min, overscroll, viewport, extent) = match horizontal {
            true => (offset.x, min.x, over.x, self.viewport.width, self.extent.width),
            false => (offset.y, min.y, over.y, self.viewport.height, self.extent.height),
        };
        // An axis that does not scroll has nothing to indicate.
        let (progress, ratio) = if min < 0.0 { (offset / min, viewport / extent) } else { (0.0, 1.0) };
        Some(BarView { horizontal, progress, ratio, overscroll })
    }

    /// The track of a bar, where it starts and how long it is along the scroll axis, and the
    /// thumb on it for a content that is not pulled past an edge. Pixels, the scroll's own space.
    fn bar_track<'a>(&self, tree: &'a Tree, horizontal: bool) -> Option<(&'a SkiaScrollBar, Rect, f32, f32, (f32, f32))> {
        let bar = self.bars[horizontal as usize]?;
        let (control, base) = (tree.find::<SkiaScrollBar>(bar)?, tree.base(bar)?);
        let view = BarView { overscroll: 0.0, ..self.bar_view(bar)? };
        let track = control.track(base.rect, self.scale, horizontal);
        let (start, length) = if horizontal { (track.left, track.width()) } else { (track.top, track.height()) };
        let thumb = control.thumb(length, self.scale, &view).filter(|thumb| base.p.is_visible && length > thumb.1)?;
        Some((control, track, start, length, thumb))
    }

    /// DrawnUI SkiaScrollBar.BeginDrag: a press on a draggable bar, its grab padding included,
    /// holds the thumb where it was taken, or by its middle when the track was pressed.
    fn grab(&self, tree: &Tree, horizontal: bool, point: Point) -> Option<(bool, f32)> {
        let (bar, track, start, _, (offset, thumb)) = self.bar_track(tree, horizontal)?;
        let pad = bar.p.grab_padding * self.scale;
        let hit = match horizontal {
            true => Rect::new(track.left, track.top - pad, track.right, track.bottom + pad),
            false => Rect::new(track.left - pad, track.top, track.right + pad, track.bottom),
        };
        if !bar.p.is_draggable || !hit.contains(point) {
            return None;
        }
        let at = if horizontal { point.x } else { point.y } - start;
        Some((horizontal, if at >= offset && at <= offset + thumb { at - offset } else { thumb / 2.0 }))
    }

    /// Moves the content to where the dragged thumb is held now (DrawnUI ApplyScrollBarDrag).
    fn drag_to(&mut self, cx: &mut GestureCx) {
        let Some((horizontal, held)) = self.drag else { return };
        let Some((_, _, start, length, (_, thumb))) = self.bar_track(cx.tree, horizontal) else { return };
        let at = if horizontal { cx.point.x } else { cx.point.y } - start;
        let progress = ((at - held) / (length - thumb)).clamp(0.0, 1.0);
        let (before, min) = (self.offset, if horizontal { self.min.x } else { self.min.y });
        *self.along(horizontal) = progress * min;
        self.scrolled |= self.offset != before;
        self.apply(cx.tree, cx.id);
    }

    /// DrawnUI ProcessScrollBarGestures: a gesture that starts on a draggable bar is the bar's
    /// until the next down: the content neither pans nor gets the tap. True when it was taken.
    fn bar_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> bool {
        if gesture.kind == GestureKind::Down {
            self.drag = [false, true].into_iter().find_map(|horizontal| self.grab(cx.tree, horizontal, cx.point));
            self.bar_owns_gesture = self.drag.is_some();
            if self.drag.is_some() {
                (self.order, self.snap_due) = (None, false);
                self.stop_scrolling();
            }
        }
        if !self.bar_owns_gesture {
            return false;
        }
        if self.drag.is_none() {
            // Released already: only the tap that follows belongs to the bar.
            return gesture.kind == GestureKind::Tapped;
        }
        match gesture.kind {
            GestureKind::Down | GestureKind::Panning => self.drag_to(cx),
            GestureKind::Up => {
                self.drag = None;
                // A tick: the bars learn that nothing holds them anymore.
                self.apply(cx.tree, cx.id);
            }
            _ => {}
        }
        true
    }

    /// For a tick, when there are bars: whether they have something new to show, whether the
    /// scroll is moving, and which axes have somewhere to go.
    fn bars_state(&mut self) -> Option<(bool, bool, [bool; 2])> {
        if self.bars == [None; 2] {
            return None;
        }
        let scrolling = self.is_animating() || self.is_user_panning || self.drag.is_some();
        let seen = (self.offset, self.extent, self.viewport, scrolling);
        // The first look is the scroll as it opens: the bars stay hidden until something changes.
        let changed = self.bars_seen.replace(seen).is_some_and(|before| before != seen);
        Some((changed, scrolling, [self.min.y < 0.0, self.min.x < 0.0]))
    }

    /// Keeps one axis inside the content. Along the scroll axis, with `bounces`, the part past an
    /// edge is not cut but shortened by the rubber band (DrawnUI RubberBandClamp).
    fn clamp(&self, horizontal: bool, value: f32, strict: bool) -> f32 {
        let (min, scrolls) = match horizontal {
            true => (self.min.x, self.p.orientation.along_x()),
            false => (self.min.y, self.p.orientation.along_y()),
        };
        let hard = value.clamp(min, 0.0);
        if strict || !self.p.bounces || !scrolls || value == hard {
            return hard;
        }
        // Over the viewport, points (React ClampOnTrack; C# stretches over 100 points times the
        // scale, and further for a pull to refresh).
        let over = value - hard;
        let mut dim = if horizontal { self.viewport.width } else { self.viewport.height } / self.scale;
        if dim == 0.0 {
            dim = RUBBER_ON_EMPTY;
        }
        hard + over.signum() * (1.0 - 1.0 / (over.abs() * self.p.rubber_effect / dim + 1.0)) * dim
    }

    fn stop_scrolling(&mut self) {
        self.x.motion = None;
        self.y.motion = None;
    }

    /// A pointer went down, or a pan starts over: the content stops and the pan counts from here.
    fn reset_pan(&mut self) {
        self.is_user_focused = true;
        self.is_user_panning = false;
        self.snap_due = false;
        self.child_was_panning = false;
        self.stop_scrolling();
        self.accumulator.clear();
        self.panning_last_delta = Point::default();
        self.panning_offset = self.offset;
    }

    fn start_ticking(&mut self, tree: &mut Tree, id: ControlId) {
        if !std::mem::replace(&mut self.ticking, true) {
            animators::start_frame(tree, id, tick);
        } else {
            // Asleep: until its bars hide, or until something happens.
            animators::sleep(tree, id, 0.0);
        }
    }

    /// Writes the offset into the tree and asks for a tick, where the handlers run.
    fn apply(&mut self, tree: &mut Tree, id: ControlId) {
        self.check_edges = true;
        self.placement().write(tree, id);
        self.start_ticking(tree, id);
    }

    fn scroll_to(&mut self, to: Point, ms: f32) {
        self.stop_scrolling();
        let to = self.inside(to);
        if ms <= 0.0 {
            self.offset = to;
            return;
        }
        let rate = rate(self.p.friction_scrolled);
        for (horizontal, from, to) in [(true, self.offset.x, to.x), (false, self.offset.y, to.y)] {
            if from != to {
                self.axis(horizontal).start(Motion::Range(Range::new(from, to, ms, rate)));
            }
        }
    }

    /// DrawnUI StartToFlingFrom: a fling that would end past an edge is cut to stop at the edge.
    fn start_fling(&mut self, horizontal: bool, velocity: f32) -> bool {
        let (from, min) = if horizontal { (self.offset.x, self.min.x) } else { (self.offset.y, self.min.y) };
        let curve = Deceleration::new(from, velocity, rate(self.p.friction_scrolled));
        let destination = curve.destination();
        let edge = (destination < min || destination > 0.0).then(|| destination.clamp(min, 0.0));
        let secs = edge.map_or(curve.duration_secs(), |edge| curve.duration_to_value(edge));
        if secs <= 0.0 {
            return false;
        }
        let (last_value, last_secs, slow_frames) = (from as f64, 0.0, 0);
        let fling = Fling { curve, secs, edge, velocity, last_value, last_secs, slow_frames };
        self.axis(horizontal).start(Motion::Fling(fling));
        true
    }

    fn bounce(&mut self, horizontal: bool, to: f32, velocity: f32) {
        let displacement = *self.along(horizontal) - to;
        if displacement != 0.0 || velocity != 0.0 {
            let velocity = velocity.clamp(-self.p.max_bounce_velocity, self.p.max_bounce_velocity);
            let bounce = Bounce::new(to, displacement, velocity, self.p.rubber_damping);
            self.axis(horizontal).start(Motion::Bounce(bounce));
        }
    }

    /// One frame of the motion of one axis.
    fn animate(&mut self, horizontal: bool, time_ms: f64) {
        let axis = self.axis(horizontal);
        let Some(mut motion) = axis.motion.take() else { return };
        let start = *axis.start_ms.get_or_insert(time_ms);
        let secs = ((time_ms - start) / 1000.0) as f32;
        let dt = secs - std::mem::replace(&mut axis.last_secs, secs);
        let (value, done) = match &mut motion {
            Motion::Fling(fling) => fling.update(dt, secs),
            Motion::Bounce(bounce) => bounce.update(secs),
            Motion::Range(range) => range.update(dt, secs),
            Motion::Step(step) => step.update(secs),
        };
        // A fling never leaves the content by more than the rubber band lets it.
        let value = if matches!(motion, Motion::Fling(_)) { self.clamp(horizontal, value, false) } else { value };
        *self.along(horizontal) = value;
        if !done {
            self.axis(horizontal).motion = Some(motion);
        } else if let Motion::Fling(Fling { edge: Some(edge), velocity, .. }) = motion
            && self.p.bounces
        {
            // Cut at the edge: what is left of its speed goes into a bounce (DrawnUI BounceIfNeeded).
            let velocity = velocity.clamp(-self.p.max_bounce_velocity, self.p.max_bounce_velocity);
            if velocity.abs() > THRESHOLD_SWIPE_ON_UP * self.scale {
                self.bounce(horizontal, edge, velocity);
            }
        }
    }

    /// The content above the rows on screen grew by `points` (DrawnUI OffsetVisibleAnchorY): the
    /// rows stay where they are when the offset goes back by as much. Everything that holds an
    /// offset goes with it, or the next pan move, or the next frame of a fling, would undo it.
    fn shift_anchor(&mut self, points: f32) {
        self.offset.y -= points;
        self.panning_offset.y -= points;
        match &mut self.y.motion {
            Some(Motion::Fling(fling)) => {
                fling.curve.value -= points as f64;
                fling.last_value -= points as f64;
            }
            Some(Motion::Bounce(bounce)) => bounce.origin -= points,
            Some(Motion::Range(range)) => {
                range.curve.value -= points as f64;
                range.last_value -= points as f64;
                range.to -= points;
            }
            Some(Motion::Step(step)) => {
                step.from -= points;
                step.to -= points;
            }
            None => {}
        }
    }

    /// A fling cut at an edge was planned for the content as it was. After a shift it goes on from
    /// where it is with the speed it has, against the content as it is now: rows that came in above
    /// the start are flung into, not stopped at (DrawnUI _replanFlingY).
    fn replan_fling(&mut self) {
        if let Some(Motion::Fling(Fling { edge: Some(_), velocity, .. })) = self.y.motion {
            self.y.motion = None;
            if velocity.abs() > MIN_VELOCITY && self.start_fling(false, velocity) {
                // It has this speed since the last tick.
                self.y.start_ms = Some(self.tick_ms);
            }
        }
    }

    /// Takes the sizes of an arrange: the content, what scrolls in all (`extent`), the viewport.
    /// True when the offset had to move to stay inside.
    fn set_bounds(&mut self, content: Size, extent: Size, viewport: Size) -> bool {
        if content == self.content && extent == self.extent && viewport == self.viewport {
            return false;
        }
        (self.content, self.extent, self.viewport) = (content, extent, viewport);
        let (orientation, scale) = (self.p.orientation, self.scale);
        // How far the content can go, points.
        let travel = |extent: f32, viewport: f32| -(extent - viewport).max(0.0) / scale;
        self.min.x = if orientation.along_x() { travel(extent.width, viewport.width) } else { 0.0 };
        self.min.y = if orientation.along_y() { travel(extent.height, viewport.height) } else { 0.0 };
        // Other content: the edges are looked at again.
        self.check_edges = true;
        // An offset the new bounds no longer contain comes back inside, unless a gesture or an
        // animation holds it past the edge on purpose.
        let inside = self.inside(self.offset);
        let held = self.is_user_panning || self.is_animating() || self.refreshing;
        let moves = !held && inside != self.offset;
        if moves {
            self.offset = inside;
        }
        moves
    }

    /// One step of an open `scroll_to_index` (DrawnUI ExecuteScrollToIndexOrder). `aim` is the
    /// offset that shows the row by the sizes the list knows now, and whether the row itself is
    /// measured. The viewport goes there; rows that get their size on the way move the target, and
    /// the order stays open until the row is measured and the viewport stands on it.
    fn pursue(&mut self, aim: Option<(f32, bool)>) {
        let Some(mut order) = self.order else { return };
        let (before, mut done) = (self.offset.y, false);
        if let Some((y, measured)) = aim {
            let target = self.inside(Point::new(self.offset.x, y));
            if let Some(Motion::Range(range)) = self.y.motion {
                // On the way: the animation goes on to the new target in the time it has left.
                if range.to != target.y {
                    let (left, rate) = ((range.secs - self.y.last_secs).max(0.016) * 1000.0, range.curve.rate);
                    self.y.start(Motion::Range(Range::new(self.offset.y, target.y, left, rate)));
                    self.y.start_ms = Some(self.tick_ms);
                }
            } else if (self.offset.y - target.y).abs() <= 0.5 {
                (self.offset.y, done) = (target.y, measured);
            } else {
                self.scroll_to(target, order.ms);
                order.ms = 0.0;
            }
        }
        // No list yet, or a row that never gets a size: not forever.
        order.stalled = if self.offset.y == before && !self.is_animating() { order.stalled + 1 } else { 0 };
        self.order = (!done && order.stalled < 10).then_some(order);
    }

    /// React CheckLoadMore: which handlers must run now (end, start). The end calls when the
    /// viewport is within its offset of it, or there is nothing to scroll; then not again until the
    /// content changed, or the user went 100 points away from the end for 2 s. The start calls when
    /// the viewport comes back to it after having been 100 points away. Over a templated list the
    /// content changed when its item count did (`items`): its length also changes whenever it
    /// measures rows it had only estimated, which is no new content, and would call the end again
    /// while the page asked for is still on its way.
    fn edges_due(&mut self, time_ms: f64, items: Option<usize>) -> (bool, bool) {
        let mut due = (false, false);
        // An open order has the viewport on its way somewhere: the edges are looked at after it.
        if !self.check_edges || self.order.is_some() {
            return due;
        }
        self.check_edges = false;
        let (offset, min, extent) = match self.p.orientation {
            ScrollOrientation::Vertical => (self.offset.y, self.min.y, self.extent.height / self.scale),
            ScrollOrientation::Horizontal => (self.offset.x, self.min.x, self.extent.width / self.scale),
            _ => return due,
        };
        let (zone, top_zone) = (self.p.load_more_offset, self.p.load_more_top_offset);
        if self.on_load_more.is_some() {
            let (underfills, content) = (min >= 0.0, items.map_or(extent, |items| items as f32));
            if let Some((at, ms)) = self.load_more_at
                && (at != content || (!underfills && offset - min > zone + 100.0 && time_ms - ms > 2000.0))
            {
                self.load_more_at = None;
            }
            if self.load_more_at.is_none() && (underfills || offset <= min + zone) {
                (self.load_more_at, due.0) = (Some((content, time_ms)), true);
            }
        }
        if self.on_load_more_top.is_some() {
            if -offset > top_zone + 100.0 {
                self.load_more_top_armed = true;
            } else if self.load_more_top_armed && min < 0.0 && offset >= -top_zone {
                (self.load_more_top_armed, due.1) = (false, true);
            }
        }
        due
    }

    /// React SkiaScroll.ProcessGestures, Panning: the first move along the axis that is fast
    /// enough starts the pan (`IgnoreWrongDirection`: not one that went further across than
    /// along); from then on every move goes into the offset, through the rubber band past the
    /// edges. A refresh does not hold the content (C# does).
    fn pan(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        let orientation = self.p.orientation;
        if !self.p.responds_to_gestures || !self.had_down || orientation == ScrollOrientation::Neither {
            return Handled::No;
        }
        self.velocity = gesture.velocity * (1.0 / self.scale);
        if !self.is_user_panning {
            if self.child_was_panning {
                return Handled::No;
            }
            let (x, y) = (gesture.total.x.abs(), gesture.total.y.abs());
            let threshold = SCROLL_VELOCITY_THRESHOLD * self.scale;
            let wrong = match orientation {
                ScrollOrientation::Vertical => x > y && x > threshold,
                ScrollOrientation::Horizontal => y > x && y > threshold,
                _ => false,
            };
            if self.p.ignore_wrong_direction && wrong {
                self.is_user_focused = false;
                return Handled::No;
            }
            let along = match orientation {
                ScrollOrientation::Vertical => self.velocity.y.abs(),
                ScrollOrientation::Horizontal => self.velocity.x.abs(),
                _ => self.velocity.x.abs().max(self.velocity.y.abs()),
            };
            if along <= SCROLL_VELOCITY_THRESHOLD {
                return Handled::No;
            }
        }
        if !self.is_user_focused {
            self.reset_pan();
        }
        self.is_user_panning = true;
        // The content went back since the last refresh: a pull may start the next one.
        self.was_refreshing &= self.overscroll_distance().y > 0.0;
        self.snap_due = self.p.snap_to_children != SnapToChildrenType::Disabled;
        self.accumulator.capture(self.velocity, gesture.time_ms);

        let (before, moved) = (self.offset, gesture.delta * (self.p.change_distance_panned / self.scale));
        let step = self.panning_last_delta + (moved - self.panning_last_delta) * PAN_SMOOTHING;
        self.panning_last_delta = step;
        self.panning_offset += step;
        if orientation != ScrollOrientation::Vertical {
            self.offset.x = self.clamp(true, self.panning_offset.x, false);
        }
        if orientation != ScrollOrientation::Horizontal {
            self.offset.y = self.clamp(false, self.panning_offset.y, false);
        }
        self.scrolled |= self.offset != before;
        // React CheckNeedRefresh: pulled far enough, a refresh starts, once per pull.
        let limit = self.p.refresh_distance_limit.max(self.p.refresh_show_distance);
        let can = self.p.refresh_enabled && orientation == ScrollOrientation::Vertical && !self.was_refreshing && !self.refreshing;
        if can && self.slots.indicator.is_some() && self.on_refresh.is_some() && self.offset.y > limit {
            self.p.is_refreshing = true;
            self.sync_refresh(gesture.time_ms);
        }
        self.apply(cx.tree, cx.id);
        Handled::Yes
    }

    /// React SkiaScroll.ProcessGestures, Up: past an edge the content springs back (to
    /// `refresh_show_distance` while it refreshes); a release faster than the swipe threshold on
    /// either axis flings along the axes that scroll; a pan that ends without a fling is snapped.
    fn release(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        self.had_down = false;
        let was_panning = std::mem::take(&mut self.is_user_panning);
        let orientation = self.p.orientation;
        if !self.p.responds_to_gestures || orientation == ScrollOrientation::Neither {
            return Handled::No;
        }
        self.is_user_focused = false;
        let over = self.overscroll_distance();
        if std::mem::take(&mut self.child_was_panning) || (!was_panning && over.is_zero()) {
            self.accumulator.clear();
            return Handled::No;
        }
        // A press that was taken away settles in place: the moves before it start no fling.
        let speed = self.accumulator.final_velocity(cx.tree.time_ms, self.p.max_velocity);
        let released = if gesture.cancelled { Point::default() } else { speed };
        self.accumulator.clear();
        let velocity = released * self.p.change_velocity_scrolled;

        if !over.is_zero() {
            for (horizontal, over, velocity) in [(true, over.x, velocity.x), (false, over.y, velocity.y)] {
                if over != 0.0 {
                    let mut edge = *self.along(horizontal) - over;
                    if self.refreshing && !horizontal && over > 0.0 {
                        edge = self.p.refresh_show_distance;
                    }
                    self.bounce(horizontal, edge, velocity);
                }
            }
            self.apply(cx.tree, cx.id);
            return Handled::Yes;
        }
        let threshold = THRESHOLD_SWIPE_ON_UP * self.scale;
        let swipe = velocity.x.abs() > threshold || velocity.y.abs() > threshold;
        let mut fling = false;
        if swipe {
            if orientation != ScrollOrientation::Vertical && velocity.x.abs() > MIN_VELOCITY {
                fling |= self.start_fling(true, velocity.x);
            }
            if orientation != ScrollOrientation::Horizontal && velocity.y.abs() > MIN_VELOCITY {
                fling |= self.start_fling(false, velocity.y);
            }
        }
        if !fling && was_panning && self.may_snap() {
            self.snap_due = false;
            let position = self.snap_position();
            self.snap(hit(self, cx.tree, cx.id, position));
        }
        self.apply(cx.tree, cx.id);
        if fling || was_panning { Handled::Yes } else { Handled::No }
    }

    /// DrawnUI ApplyWheelScroll: a notch moves `WHEEL_LINE_SIZE` points, a touchpad event its
    /// share, from where a running step goes, so a fast spin travels all its notches; no bounce.
    /// An event under half a notch (a touchpad, a free-spinning or high-resolution wheel) moves the
    /// content at once, as a browser scrolls a page; only a notch glides (DrawnUi.React c90cc45:
    /// easing every small event kept the content behind the fingers).
    /// As React for the order of nested scrolls: a scroll that cannot move that way does not use
    /// the wheel, the scroll around this one (or the page) may take it.
    /// The axis (C# ba03cb20): a vertical scroll leaves horizontal events, so the sideways part of
    /// a diagonal swipe does not step a vertical list; a horizontal scroll takes both kinds.
    fn wheel(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        let orientation = self.p.orientation;
        if !self.p.responds_to_gestures || orientation == ScrollOrientation::Neither {
            return Handled::No;
        }
        let horizontal = match orientation {
            ScrollOrientation::Vertical if gesture.wheel_horizontal => return Handled::No,
            ScrollOrientation::Horizontal => true,
            // Both ways: each event on its own axis (C# does not move a two-way scroll).
            _ => gesture.wheel_horizontal,
        };
        let from = match self.axis(horizontal).motion {
            Some(Motion::Step(step)) => step.to,
            _ => *self.along(horizontal),
        };
        let min = if horizontal { self.min.x } else { self.min.y };
        if (gesture.wheel < 0.0 && from <= min) || (gesture.wheel > 0.0 && from >= 0.0) || gesture.wheel == 0.0 {
            return Handled::No;
        }
        self.order = None;
        let to = (from + WHEEL_LINE_SIZE * gesture.wheel).clamp(min, 0.0);
        self.stop_scrolling();
        let current = *self.along(horizontal);
        if gesture.wheel.abs() < 0.5 {
            // No snap after a move that comes at once (React snaps when a motion comes to rest).
            *self.along(horizontal) = to;
        } else {
            self.snap_due = self.p.snap_to_children != SnapToChildrenType::Disabled;
            if to != current {
                let ms = self.p.scrolling_speed_ms;
                let axis = self.axis(horizontal);
                axis.start(Motion::Step(Step { from: current, to, ms }));
                // From the event's time, not the next frame's: a fast swipe sends an event before
                // every frame, and each frame drew a fresh glide at progress 0, so the content
                // stood still until the events thinned out.
                axis.start_ms = Some(gesture.time_ms.max(cx.tree.time_ms));
            }
        }
        self.apply(cx.tree, cx.id);
        Handled::Yes
    }
}

/// The list a scroll looks into for rows: its content, or the first list below it.
fn find_list(tree: &Tree, content: ControlId) -> Option<ControlId> {
    let is_list = |id: ControlId| tree.find::<SkiaLayout>(id).is_some_and(|layout| layout.items.is_some());
    if is_list(content) {
        return Some(content);
    }
    tree.trackers.iter().copied().find(|tracker| tree.is_ancestor(content, *tracker) && is_list(*tracker))
}

/// The item count of the list a scroll with an end handler looks into, as the list was last laid
/// out for; `None` without a handler or a list.
fn list_items(tree: &Tree, id: ControlId) -> Option<usize> {
    let scroll = tree.find::<SkiaScroll>(id).filter(|scroll| scroll.on_load_more.is_some())?;
    let list = find_list(tree, slot(tree, id, scroll.slots.content)?)?;
    Some(tree.find::<SkiaLayout>(list)?.items_count())
}

/// For an order: the viewport offset (points) that puts the row where the order wants it, by the
/// sizes the list knows now, and whether the row is measured. `None` without a list or a row.
fn aim(tree: &Tree, id: ControlId, order: Order) -> Option<(f32, bool)> {
    let scroll = tree.find::<SkiaScroll>(id)?;
    let content = slot(tree, id, scroll.slots.content)?;
    let list = find_list(tree, content)?;
    let layout = tree.find::<SkiaLayout>(list)?;
    if order.index >= layout.items_count() {
        return None;
    }
    // From the top of the content: the list may sit below other things in a stack.
    let row = tree.base(list)?.rect.top - tree.base(content)?.rect.top + layout.item_offset_pixels(order.index);
    let room = scroll.viewport.height - layout.item_height_pixels(order.index);
    let pixels = match order.position {
        RelativePositionType::End => row - room,
        RelativePositionType::Center => row - room / 2.0,
        _ => row,
    };
    Some((-pixels / scroll.scale, layout.is_item_measured(order.index)))
}

/// React GetIndexHit: the child of the content under a point of the viewport, `point` pixels
/// from its start along the scroll axis: its index (the item index of a recycled cell), its
/// start and its end along the axis in layout pixels (the offset not applied). A child that starts
/// on the point counts, one that ends there does not; the first one wins. Nothing is looked at
/// across the axis.
// ponytail: looks at every child of the content when the offset changed; a long plain stack pays
// for it, a list has only its realized cells as children.
fn tracked(tree: &Tree, id: ControlId, position: RelativePositionType) -> Option<(usize, f32, f32)> {
    hit(tree.find::<SkiaScroll>(id)?, tree, id, position)
}

/// `tracked` for a scroll that is not in its slot (its own hooks run).
fn hit(scroll: &SkiaScroll, tree: &Tree, id: ControlId, position: RelativePositionType) -> Option<(usize, f32, f32)> {
    let (at, horizontal) = (scroll.viewport_point(position)?, scroll.horizontal());
    let content = slot(tree, id, scroll.slots.content)?;
    // Where the children are drawn: the viewport start plus the point, less the pixels they are moved by.
    let pixels = scroll.pixel_offset();
    let point = if horizontal { scroll.viewport_origin.x + at - pixels.x } else { scroll.viewport_origin.y + at - pixels.y };
    for (index, child) in tree.children(content).iter().enumerate() {
        let Some(child) = tree.base(*child) else { continue };
        let r = child.rect;
        if !child.p.is_visible || r.width() <= 0.0 || r.height() <= 0.0 {
            continue;
        }
        let (start, end) = if horizontal { (r.left, r.right) } else { (r.top, r.bottom) };
        if point >= start && point < end {
            return Some((child.context_index.unwrap_or(index), start, end));
        }
    }
    None
}

/// Runs a handler with the scroll as `me`. Like a tapped handler: the node leaves its slot
/// meanwhile, so the handler can reach the rest of the tree.
pub(crate) fn fire(cx: &mut Cx<'_>, id: ControlId, handler: impl FnOnce(Raw<'_>, &mut Cx<'_>)) {
    let Some(mut node) = cx.tree.take(id) else { return };
    let mut queue = Vec::new();
    if let Some(control) = node.kind.as_deref_mut() {
        handler(Raw { id, control, base: &mut node.base, queue: &mut queue }, &mut Cx { tree: cx.tree });
    }
    cx.tree.put_back(node);
    cx.tree.queue.append(&mut queue);
}

/// One frame of a scroll: motion, an open order, then the handlers. Runs while something moves,
/// and once after every change from a gesture, from code or from layout.
fn tick(id: ControlId, time_ms: f64, state: &mut dyn Any, cx: &mut Cx<'_>) -> FrameTick {
    let mut tick = FrameTick { keep: false, state_touched: false };
    let Some(mut me) = cx.tree.find_mut::<SkiaScroll>(id) else { return tick };
    let scroll = me.control_mut();
    let before = scroll.offset;
    scroll.tick_ms = time_ms;
    scroll.animate(true, time_ms);
    scroll.animate(false, time_ms);
    scroll.scrolled |= scroll.offset != before;
    let order = scroll.order;
    // React TrackIndex: the tracked child is looked up when the offset changed, not before.
    let tracks = scroll.p.track_index_position != RelativePositionType::None || scroll.current.is_some();
    let look_at = Some(scroll.offset).filter(|offset| tracks && scroll.index_at.is_some_and(|at| at != *offset));
    // Not tracking: turned on later, it waits for the next move, as React.
    if scroll.index_at.is_none() || !tracks {
        scroll.index_at = Some(scroll.offset);
    }
    let (track_position, snap_position) = (scroll.p.track_index_position, scroll.snap_position());
    let snap_check = scroll.snap_due && scroll.may_snap();

    // What the tree says, read while nothing of the scroll is borrowed.
    let aim = order.and_then(|order| aim(cx.tree, id, order));
    let current = look_at.map(|offset| (offset, tracked(cx.tree, id, track_position)));
    let snap_hit = if snap_check { tracked(cx.tree, id, snap_position) } else { None };
    let items = list_items(cx.tree, id);

    let Some(mut me) = cx.tree.find_mut::<SkiaScroll>(id) else { return tick };
    let scroll = me.control_mut();
    scroll.pursue(aim);
    let index = scroll.current_index();
    if let Some((offset, current)) = current {
        (scroll.current, scroll.index_at) = (current, Some(offset));
    }
    let index = Some(scroll.current_index()).filter(|now| *now != index);
    // React: the snap is looked at in the frame a motion came to rest, or a pan ended without one.
    if snap_check && !scroll.is_animating() {
        scroll.snap_due = false;
        scroll.snap(snap_hit);
    }
    scroll.check_edges |= scroll.offset != before;
    let (offset, placement) = (scroll.offset, scroll.placement());
    let (bars, bars_state, keep) = (scroll.bars, scroll.bars_state(), scroll.p.keep_scroll_bars_visible);
    let (more, more_top) = scroll.edges_due(time_ms, items);
    // The handlers leave the control while they run.
    let mut on_scrolled = if std::mem::take(&mut scroll.scrolled) { scroll.on_scrolled.take() } else { None };
    let mut on_load_more = if more { scroll.on_load_more.take() } else { None };
    let mut on_load_more_top = if more_top { scroll.on_load_more_top.take() } else { None };
    let mut on_index_changed = if index.is_some() { scroll.on_index_changed.take() } else { None };
    let mut on_refresh = if std::mem::take(&mut scroll.refresh_due) { scroll.on_refresh.take() } else { None };
    placement.write(cx.tree, id);
    // Shown while something changes, hidden some time after: the frame time the bars need next.
    let mut bars_wake: Option<f64> = None;
    if let Some((changed, scrolling, travels)) = bars_state {
        for (bar, travels) in bars.into_iter().zip(travels) {
            if let Some(mut bar) = bar.and_then(|bar| cx.tree.find_mut::<SkiaScrollBar>(bar)) {
                let wake = scroll_bar::step(&mut bar, time_ms, changed, scrolling, keep, travels);
                bars_wake = [bars_wake, wake].into_iter().flatten().min_by(f64::total_cmp);
            }
        }
    }
    if let Some(on_scrolled) = &mut on_scrolled {
        fire(cx, id, |me, cx| on_scrolled(me, &mut *state, cx, offset));
    }
    for on_load_more in [&mut on_load_more, &mut on_load_more_top, &mut on_refresh].into_iter().flatten() {
        fire(cx, id, |me, cx| on_load_more(me, &mut *state, cx));
    }
    if let (Some(on_index_changed), Some(index)) = (&mut on_index_changed, index) {
        fire(cx, id, |me, cx| on_index_changed(me, &mut *state, cx, index));
    }
    tick.state_touched = on_scrolled.is_some() || on_load_more.is_some() || on_load_more_top.is_some() || on_index_changed.is_some();
    tick.state_touched |= on_refresh.is_some();

    let Some(mut me) = cx.tree.find_mut::<SkiaScroll>(id) else { return tick };
    let scroll = me.control_mut();
    if on_scrolled.is_some() {
        scroll.on_scrolled = on_scrolled;
    }
    if on_load_more.is_some() {
        scroll.on_load_more = on_load_more;
    }
    if on_load_more_top.is_some() {
        scroll.on_load_more_top = on_load_more_top;
    }
    if on_index_changed.is_some() {
        scroll.on_index_changed = on_index_changed;
    }
    if on_refresh.is_some() {
        scroll.on_refresh = on_refresh;
    }
    let snaps = scroll.snap_due && !scroll.is_user_focused;
    let awake = scroll.is_animating() || scroll.check_edges || scroll.order.is_some() || snaps;
    // The frame animator lives as long as the scroll, asleep when nothing is to do: no frames,
    // and the scroll keeps its place before the timers and animations started after it. So its
    // edges are looked at before app code of the same frame changes the content by a timer (React
    // looks at them during layout, before the next timer fires); a new animator per wake-up would
    // come after them and call the end again for content that was just added.
    (scroll.ticking, tick.keep) = (true, true);
    if !awake {
        match bars_wake {
            // Only the bars are waiting, to hide: no frames until then.
            Some(wake) if wake > time_ms => animators::sleep(cx.tree, id, wake),
            Some(_) => {}
            None => animators::sleep(cx.tree, id, f64::INFINITY),
        }
    }
    tick
}

impl Has<ScrollProps> for SkiaScroll {
    fn part(&self) -> &ScrollProps {
        &self.p
    }
    fn part_mut(&mut self) -> &mut ScrollProps {
        &mut self.p
    }
}

impl Control for SkiaScroll {
    fn moves_content(&self) -> Option<bool> {
        Some(self.is_animating() || self.is_user_panning)
    }
    /// A node of its own in a screen reader's tree, which pages it (`Ui::accessibility_scroll`);
    /// the browser overlay leaves it out.
    fn accessibility_role(&self) -> Option<&'static str> {
        Some(crate::ui::Aria::SCROLL_VIEW)
    }

    /// Its colors for the bars go into them, `is_refreshing` is acted on. A tick tells the bars
    /// about `keep_scroll_bars_visible`, looks the tracked child up again and writes the offset.
    fn on_props_changed(&mut self, cx: &mut Cx) {
        for bar in self.bars.into_iter().flatten() {
            let Some(mut bar) = cx.tree.find_mut::<SkiaScrollBar>(bar) else { continue };
            if let Some(color) = self.p.scroll_bar_thumb_color {
                bar.set_thumb_color(color);
            }
            if let Some(color) = self.p.scroll_bar_track_color {
                bar.set_track_color(color);
            }
        }
        self.sync_refresh(cx.tree.time_ms);
        if let Some(id) = self.id {
            self.start_ticking(cx.tree, id);
        }
    }

    /// The content is as long as it needs along the scroll axis, whatever the viewport is. Header
    /// and footer are measured like the scroll itself and add to the content along the axis.
    fn measure(&mut self, cx: &mut LayoutCx, width: f32, height: f32) -> Size {
        let (inner, added) = layout::content_box(&cx.base().p, width, height, cx.scale);
        let (along_x, along_y) = (self.p.orientation.along_x(), self.p.orientation.along_y());
        let mut size = Size::default();
        if let Some(content) = visible(cx, self.slots.content) {
            let width = if along_x { f32::INFINITY } else { inner.width };
            size = cx.measure_child(content, width, if along_y { f32::INFINITY } else { inner.height });
        }
        for extra in [self.slots.header, self.slots.footer] {
            if let Some(extra) = visible(cx, extra) {
                let extra = cx.measure_child(extra, inner.width, inner.height);
                if self.horizontal() {
                    size.width += extra.width;
                } else {
                    size.height += extra.height;
                }
            }
        }
        // The bars and the refresh indicator lie over the viewport and take no room.
        for over in self.bars.into_iter().flatten().chain(visible(cx, self.slots.indicator)) {
            cx.measure_child(over, inner.width, inner.height);
        }
        Size::new(size.width + added.width, size.height + added.height)
    }

    fn arrange(&mut self, cx: &mut LayoutCx) {
        (self.scale, self.id) = (cx.scale, Some(cx.id));
        // `is_refreshing` of a scroll that was built with it.
        self.sync_refresh(cx.tree.time_ms);
        let Some(child) = slot(cx.tree, cx.id, self.slots.content) else { return };
        let (scale, id, horizontal) = (cx.scale, cx.id, self.horizontal());
        let viewport = layout::content_rect(cx.base(), scale);
        self.viewport_origin = Point::new(viewport.left, viewport.top);
        let (along_x, along_y) = (self.p.orientation.along_x(), self.p.orientation.along_y());
        let measured = |cx: &LayoutCx, child: ControlId| {
            let base = cx.child_base(child);
            if base.p.is_visible { base.measured } else { Size::default() }
        };
        // Along the axis header and footer sit on: a length, and a rect that far from the start
        // of the viewport.
        let along = |size: Size| if horizontal { size.width } else { size.height };
        let place = |from: f32, size: Size| match horizontal {
            true => Rect::from_xywh(viewport.left + from, viewport.top, size.width, size.height),
            false => Rect::from_xywh(viewport.left, viewport.top + from, size.width, size.height),
        };
        let (header, footer) = (visible(cx, self.slots.header), visible(cx, self.slots.footer));
        let header_size = header.map_or(0.0, |header| along(measured(cx, header)));
        let footer_size = footer.map_or(0.0, |footer| along(measured(cx, footer)));
        let gap = self.p.content_offset * scale;
        // As upstream: the gap moves the content only under a sticky or behind header, the footer
        // under any header, and it always counts for the length.
        let covered = self.p.header_sticky || self.p.header_behind;
        let lead = if header.is_some() { header_size + if covered { gap } else { 0.0 } } else { 0.0 };
        let footer_lead = if header.is_some() { header_size + gap } else { 0.0 };
        // Another scale: the same points are other pixels, and a list below arranges by them.
        self.placement().write(cx.tree, id);

        let mut shifted = false;
        // Twice at most: again when the sizes of this arrange pulled the offset back inside the
        // content, so that a list below shows its rows for where the viewport is.
        for _ in 0..2 {
            // The content keeps its measured size along the scroll axis and gets the viewport across it.
            let content = measured(cx, child);
            let width = if along_x { content.width } else { viewport.width() };
            let height = if along_y { content.height } else { viewport.height() };
            cx.arrange_child(child, place(lead, Size::new(width, height)));
            // Content above the rows on screen changed its size (a list measured rows it had only
            // estimated, rows were inserted above): the rows on screen stay where they are.
            let shift = cx.take_viewport_shift(child);
            if shift != 0.0 {
                self.shift_anchor(shift / scale);
                shifted = true;
            }
            // Read again: a list learns the sizes of its rows while it is arranged.
            let content = measured(cx, child);
            let across = |size: Size| if horizontal { Size::new(size.width, viewport.height()) } else { Size::new(viewport.width(), size.height) };
            if let Some(header) = header {
                cx.arrange_child(header, place(0.0, across(measured(cx, header))));
                // Over the content for the pointer too, unless it is drawn behind it.
                if let Some(node) = cx.tree.node_mut(header) {
                    node.base.p.z_index = if self.p.header_behind { -1 } else { 1 };
                }
            }
            if let Some(footer) = footer {
                cx.arrange_child(footer, place(footer_lead + along(content), across(measured(cx, footer))));
            }
            let extra = header_size + footer_size + gap;
            let extent = if horizontal { Size::new(content.width + extra, content.height) } else { Size::new(content.width, content.height + extra) };
            if !self.set_bounds(content, extent, viewport.size()) {
                break;
            }
            cx.tree.set_content_offset(id, self.pixel_offset());
        }
        if shifted {
            self.replan_fling();
        }
        for bar in self.bars.into_iter().flatten() {
            cx.arrange_child(bar, viewport);
        }
        // At the start of the viewport; it shows only, as the upstream RefreshIndicator.
        let indicator = visible(cx, self.slots.indicator);
        if let Some(indicator) = indicator {
            let height = measured(cx, indicator).height;
            cx.arrange_child(indicator, Rect::from_xywh(viewport.left, viewport.top, viewport.width(), height));
            self.indicator_height = cx.child_base(indicator).rect.height() / scale;
            if let Some(node) = cx.tree.node_mut(indicator) {
                node.base.p.input_transparent = true;
            }
        }
        // The content is arranged for this offset already: nothing to lay out again.
        let placement = self.placement();
        cx.base_mut().content_offset = placement.pixels;
        if let Some(node) = header.and_then(|header| cx.tree.node_mut(header)) {
            (node.base.p.left, node.base.p.top) = (placement.header_shift.x, placement.header_shift.y);
        }
        if let Some(node) = indicator.and_then(|indicator| cx.tree.node_mut(indicator)) {
            (node.base.p.translation_y, node.base.p.opacity) = (placement.indicator_top, placement.indicator_opacity);
        }

        let tracks = self.p.track_index_position != RelativePositionType::None;
        if tracks || self.refresh_due || self.check_edges && (self.on_load_more.is_some() || self.on_load_more_top.is_some() || self.bars != [None; 2]) {
            self.start_ticking(cx.tree, id);
        }
    }

    /// The content never shows outside the scroll. A header is drawn over the content, or under
    /// it when it is `header_behind`; the refresh indicator and the scroll bars over everything.
    fn paint(&self, cx: &mut PaintCx) {
        let Some(node) = cx.node(cx.id) else { return };
        let child = |at: Option<usize>| at.and_then(|at| node.children.get(at).copied());
        let (content, header, footer) = (child(self.slots.content), child(self.slots.header), child(self.slots.footer));
        let order = if self.p.header_behind { [header, content, footer] } else { [content, footer, header] };
        cx.canvas.save();
        cx.canvas.clip_rect(cx.rect, ClipOp::Intersect, true);
        for child in order.into_iter().flatten() {
            cx.paint_child(child);
        }
        // The refresh indicator and the bars stay on the viewport: the offset that moves the
        // content does not move them.
        for over in child(self.slots.indicator).into_iter().chain(self.bars.into_iter().flatten()) {
            paint::render(cx, over);
        }
        cx.canvas.restore();
    }

    /// React SkiaScroll.ProcessGestures, for one pointer. The children go first: a press, the
    /// wheel, a pan while this scroll is not panning yet, a release. A scroll inside this one takes
    /// a pan along its axis and owns the press then; it takes the wheel while it can move that way.
    fn on_gesture(&mut self, cx: &mut GestureCx, gesture: &Gesture) -> Handled {
        if self.bar_gesture(cx, gesture) {
            return Handled::Yes;
        }
        let by = |child: Option<ControlId>| child.map_or(Handled::No, Handled::By);
        match gesture.kind {
            GestureKind::Down => {
                (self.had_down, self.snapped) = (true, false);
                if self.p.responds_to_gestures {
                    // The finger takes over from whatever was moving the content.
                    self.order = None;
                    self.reset_pan();
                }
                by(cx.route_children(gesture))
            }
            GestureKind::Panning => {
                if !self.is_user_panning && self.p.responds_to_gestures && self.had_down {
                    // A child may own the pan (a scroll or a carousel inside): asked once, before this
                    // scroll takes over, and then the pan is its for the whole press.
                    if let Some(child) = cx.route_children(gesture) {
                        self.child_was_panning = true;
                        return Handled::By(child);
                    }
                }
                self.pan(cx, gesture)
            }
            // A pan never ends in a tap.
            GestureKind::Tapped => if self.is_user_panning { Handled::Yes } else { by(cx.route_children(gesture)) },
            GestureKind::Up => {
                // The children release first (a button lets go).
                let child = cx.route_children(gesture);
                match self.release(cx, gesture) {
                    Handled::No => by(child),
                    handled => handled,
                }
            }
            GestureKind::Wheel => match cx.route_children(gesture) {
                Some(child) => Handled::By(child),
                None => self.wheel(cx, gesture),
            },
            _ => Handled::No,
        }
    }
}

/// A child of the scroll by its slot, when it is there and visible.
fn visible(cx: &LayoutCx, at: Option<usize>) -> Option<ControlId> {
    slot(cx.tree, cx.id, at).filter(|child| cx.child_base(*child).p.is_visible)
}

fn scroll_part<T: Control>(control: &mut T) -> &mut SkiaScroll {
    part_mut(control).expect("the control embeds a SkiaScroll")
}

/// A load-more handler as the scroll stores it.
fn load_more<T: Control, S: Any>(mut f: impl FnMut(&mut Mut<'_, T>, &mut S, &mut Cx<'_>) + 'static) -> LoadMore {
    Box::new(move |me, state, cx| {
        let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
        f(&mut me.typed(), state, cx)
    })
}

impl<T: Has<ScrollProps>> Build<T> {
    fn push_slot(mut self, child: impl Into<Detached>, pick: fn(&mut Slots) -> &mut Option<usize>) -> Self {
        let slots = &mut scroll_part(self.control_mut()).slots;
        *pick(slots) = Some(slots.count);
        slots.count += 1;
        self.push_child(child);
        self
    }

    /// The one child the scroll moves.
    pub fn content(self, content: impl Into<Detached>) -> Self {
        self.push_slot(content, |slots| &mut slots.content)
    }

    /// A child before the content on the scroll axis, as wide as the viewport across it (DrawnUI
    /// Header). It scrolls away with the content unless `header_sticky` keeps it; see also
    /// `header_behind` and `header_parallax_ratio`. The scroll moves it with its `left` / `top`.
    pub fn header(self, header: impl Into<Detached>) -> Self {
        self.push_slot(header, |slots| &mut slots.header)
    }

    /// A child after the content on the scroll axis (DrawnUI Footer).
    pub fn footer(self, footer: impl Into<Detached>) -> Self {
        self.push_slot(footer, |slots| &mut slots.footer)
    }

    fn push_bar(mut self, bar: Build<SkiaScrollBar>, horizontal: bool) -> Self {
        let scroll = scroll_part(self.control_mut());
        scroll.bars[horizontal as usize] = Some(bar.id());
        scroll.slots.count += 1;
        self.push_child(bar);
        self
    }

    /// The bar of the vertical axis, drawn over the viewport (DrawnUI ScrollBar). Keep a handle
    /// to it (`assign`) to change it later.
    pub fn scroll_bar(self, bar: Build<SkiaScrollBar>) -> Self {
        self.push_bar(bar, false)
    }

    /// The bar of the horizontal axis (DrawnUI ScrollBarHorizontal).
    pub fn scroll_bar_horizontal(self, bar: Build<SkiaScrollBar>) -> Self {
        self.push_bar(bar, true)
    }

    /// A default SkiaScrollBar for each axis named that has no bar yet (DrawnUI ScrollBarsVisibility).
    pub fn scroll_bars_visibility(mut self, visibility: ScrollBarVisibility) -> Self {
        for (horizontal, other) in [(false, ScrollBarVisibility::Horizontal), (true, ScrollBarVisibility::Vertical)] {
            let wanted = visibility != ScrollBarVisibility::None && visibility != other;
            if wanted && scroll_part(self.control_mut()).bars[horizontal as usize].is_none() {
                self = self.push_bar(SkiaScrollBar::new(), horizontal);
            }
        }
        self
    }

    /// Runs once per frame in which the content moved: a pan, a fling, a bounce, the wheel, a
    /// `scroll_to` (a jump too, as React). `offset` is the viewport offset in points.
    pub fn on_scrolled<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, T>, &mut S, &mut Cx<'_>, Point) + 'static) -> Self {
        scroll_part(self.control_mut()).on_scrolled = Some(Box::new(move |me, state, cx, offset| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
            f(&mut me.typed(), state, cx, offset)
        }));
        self
    }

    /// Shown over the start of the viewport while the content is pulled past its start, with
    /// `refresh_enabled` (DrawnUI RefreshIndicator): any control, as wide as the viewport. The
    /// scroll moves it with its `translation_y` and fades it with its `opacity`.
    pub fn refresh_indicator(self, indicator: impl Into<Detached>) -> Self {
        self.push_slot(indicator, |slots| &mut slots.indicator)
    }

    /// Runs when a refresh starts: a pull beyond `refresh_distance_limit`, or `is_refreshing` set
    /// by the app (DrawnUI RefreshCommand). Set `is_refreshing` back when the work is done.
    pub fn on_refresh<S: Any>(mut self, f: impl FnMut(&mut Mut<'_, T>, &mut S, &mut Cx<'_>) + 'static) -> Self {
        scroll_part(self.control_mut()).on_refresh = Some(load_more::<T, S>(f));
        self
    }

    /// Runs when another child of the content comes to the tracked point, or none is there
    /// anymore (DrawnUI IndexChanged). Needs `track_index_position`.
    pub fn on_index_changed<S: Any>(mut self, mut f: impl FnMut(&mut Mut<'_, T>, &mut S, &mut Cx<'_>, Option<usize>) + 'static) -> Self {
        scroll_part(self.control_mut()).on_index_changed = Some(Box::new(move |me, state, cx, index| {
            let state = state.downcast_mut::<S>().unwrap_or_else(|| wrong_state::<S>());
            f(&mut me.typed(), state, cx, index)
        }));
        self
    }

    /// Runs when the viewport comes within `load_more_offset` points of the end of the content,
    /// and while the content does not fill the viewport (DrawnUI LoadMoreCommand). Add content in
    /// it; it runs again for the same place only after the content size changed.
    pub fn on_load_more<S: Any>(mut self, f: impl FnMut(&mut Mut<'_, T>, &mut S, &mut Cx<'_>) + 'static) -> Self {
        scroll_part(self.control_mut()).on_load_more = Some(load_more::<T, S>(f));
        self
    }

    /// The same for the start of the content, within `load_more_top_offset` points (DrawnUI
    /// LoadMoreTopCommand), when the viewport comes back there after it was more than the offset
    /// and 100 points away; not when the scroll opens (React). Rows a list gets inserted at its
    /// start in it come in above the viewport: what is on screen stays where it is.
    pub fn on_load_more_top<S: Any>(mut self, f: impl FnMut(&mut Mut<'_, T>, &mut S, &mut Cx<'_>) + 'static) -> Self {
        scroll_part(self.control_mut()).on_load_more_top = Some(load_more::<T, S>(f));
        self
    }
}

impl Cx<'_> {
    /// Sets or, with `None`, removes the scroll's content at run time (DrawnUI `SkiaScroll.Content`).
    pub fn set_scroll_content(&mut self, scroll: impl Into<ControlId>, content: Option<Detached>) {
        self.set_scroll_slot(scroll.into(), |slots| &mut slots.content, content);
    }

    /// Sets or removes the scroll's header at run time (DrawnUI `SkiaScroll.Header`); see `header`.
    pub fn set_scroll_header(&mut self, scroll: impl Into<ControlId>, header: Option<Detached>) {
        self.set_scroll_slot(scroll.into(), |slots| &mut slots.header, header);
    }

    /// Sets or removes the scroll's footer at run time (DrawnUI `SkiaScroll.Footer`).
    pub fn set_scroll_footer(&mut self, scroll: impl Into<ControlId>, footer: Option<Detached>) {
        self.set_scroll_slot(scroll.into(), |slots| &mut slots.footer, footer);
    }

    /// Sets or removes the scroll's refresh indicator at run time (DrawnUI
    /// `SkiaScroll.RefreshIndicator`); see `refresh_indicator`.
    pub fn set_refresh_indicator(&mut self, scroll: impl Into<ControlId>, indicator: Option<Detached>) {
        self.set_scroll_slot(scroll.into(), |slots| &mut slots.indicator, indicator);
    }

    /// A slot is a child index: a new child takes the old one's place, a new slot goes last, a
    /// removed one moves the slots after it up.
    fn set_scroll_slot(&mut self, scroll: ControlId, pick: fn(&mut Slots) -> &mut Option<usize>, child: Option<Detached>) {
        let Some(at) = self.tree.find_mut::<SkiaScroll>(scroll).map(|mut s| *pick(&mut s.control_mut().slots)) else { return };
        let old = at.and_then(|at| self.tree.children(scroll).get(at).copied());
        let now = match (old, child) {
            (Some(old), Some(child)) => {
                self.replace_child(old, child);
                at
            }
            (Some(old), None) => {
                self.tree.remove_now(old);
                if let (Some(at), Some(mut me)) = (at, self.tree.find_mut::<SkiaScroll>(scroll)) {
                    let slots = &mut me.control_mut().slots;
                    for slot in [&mut slots.content, &mut slots.header, &mut slots.footer, &mut slots.indicator].into_iter().flatten() {
                        if *slot > at {
                            *slot -= 1;
                        }
                    }
                }
                None
            }
            (None, Some(child)) => {
                let id = self.add_child(scroll, child);
                self.tree.children(scroll).iter().position(|c| *c == id)
            }
            (None, None) => return,
        };
        if let Some(mut me) = self.tree.find_mut::<SkiaScroll>(scroll) {
            *pick(&mut me.control_mut().slots) = now;
        }
        self.tree.invalidate(scroll, crate::types::Dirty::MEASURE);
    }

    /// Runs `change` on a mounted scroll, then writes its offset and starts its frames.
    fn scroll_with(&mut self, id: ControlId, change: impl FnOnce(&mut SkiaScroll)) {
        let Some(mut me) = self.tree.find_mut::<SkiaScroll>(id) else { return };
        let scroll = me.control_mut();
        let before = scroll.offset;
        change(scroll);
        // React: every offset set is reported, a jump from code too.
        scroll.scrolled |= scroll.offset != before;
        scroll.snap_due = scroll.p.snap_to_children != SnapToChildrenType::Disabled && scroll.is_animating();
        scroll.check_edges = true;
        let (placement, start) = (scroll.placement(), !std::mem::replace(&mut scroll.ticking, true));
        placement.write(self.tree, id);
        if start {
            animators::start_frame(self.tree, id, tick);
        } else {
            animators::sleep(self.tree, id, 0.0);
        }
    }

    /// Scrolls to a viewport offset in points: 0 is the start, negative values go into the
    /// content; it is kept inside the content. `ms` > 0 animates over that time, 0 jumps. Whatever
    /// moved the scroll before stops.
    pub fn scroll_to(&mut self, scroll: impl Into<ControlId>, x: f32, y: f32, ms: impl IntoProp<f32>) {
        let ms = ms.into_prop();
        self.scroll_with(scroll.into(), |scroll| {
            scroll.order = None;
            scroll.scroll_to(Point::new(x, y), ms);
        })
    }

    /// Jumps to a horizontal viewport offset in points, kept inside the content (DrawnUI
    /// ViewportOffsetX set); the vertical one stays.
    pub fn set_viewport_offset_x(&mut self, scroll: impl Into<ControlId>, x: f32) {
        self.scroll_with(scroll.into(), |scroll| {
            scroll.order = None;
            scroll.scroll_to(Point::new(x, scroll.offset.y), 0.0);
        })
    }

    /// Jumps to a vertical viewport offset in points, kept inside the content (DrawnUI
    /// ViewportOffsetY set); the horizontal one stays.
    pub fn set_viewport_offset_y(&mut self, scroll: impl Into<ControlId>, y: f32) {
        self.scroll_with(scroll.into(), |scroll| {
            scroll.order = None;
            scroll.scroll_to(Point::new(scroll.offset.x, y), 0.0);
        })
    }

    /// Scrolls so that the row of item `index` of the list inside the scroll (its content, or a
    /// list further down in it) stands at the start, the center or the end of the viewport
    /// (DrawnUI ScrollToIndex). `ms` > 0 animates. A row that was not measured yet is gone to by
    /// the sizes as they are estimated; the order stays open and is aimed again every frame until
    /// the row has its size and the viewport stands on it. A press or the wheel drops the order,
    /// so does another `scroll_to`; while it is open the load-more handlers wait.
    pub fn scroll_to_index(
        &mut self,
        scroll: impl Into<ControlId>,
        index: usize,
        position: RelativePositionType,
        ms: impl IntoProp<f32>,
    ) {
        let id = scroll.into();
        let order = Order { index, position, ms: ms.into_prop(), stalled: 0 };
        let aim = aim(self.tree, id, order);
        self.scroll_with(id, |scroll| {
            scroll.order = Some(order);
            scroll.pursue(aim);
        })
    }
}
