//! What SkiaCarousel and SkiaDrawer share (React SnappingLayout, DrawnUI SnappingLayout): a
//! content position that comes to rest on one of the `snap_points`, the anchor picked by where
//! the finger left it and how fast, reached by a spring (`bounces`) or an eased run; the rubber
//! band past the points. The controls keep a `Snapping` and move their content by its position.

use skia_safe::{Point, Rect};

use crate::animators::{Easing, easing};
use crate::controls::scroll::Bounce;
use crate::gestures::VelocityAccumulator;
use crate::props;

props!(SnappingProps, SnappingBuild, SnappingSet {
    /// A snap moves there over time; off: it jumps.
    animated / set_animated: bool = true, NONE;
    /// Off: pans do not move the content, code still does.
    responds_to_gestures / set_responds_to_gestures: bool = true, NONE;
    /// A pan that goes further across the axis than along it is left alone.
    ignore_wrong_direction / set_ignore_wrong_direction: bool = false, NONE;
    /// The content can be pulled past its snap points and springs back; a snap is a spring too.
    bounces / set_bounces: bool = false, NONE;
    /// The spring of a bouncing snap is built from it: mass 1 + it, damping ratio (1 + it) / 2.
    rubber_damping / set_rubber_damping: f32 = 0.7, NONE;
    /// Points per second, times the scale, a snap starts with when the finger left no speed.
    auto_velocity_multiply_pts / set_auto_velocity_multiply_pts: f32 = 25.0, NONE;
    /// How soft the rubber band past the snap points is.
    rubber_effect / set_rubber_effect: f32 = 0.15, NONE;
    /// The share of a step a slow pan must cover before it counts as going that way (a looped
    /// carousel at its ends).
    snap_distance_ratio / set_snap_distance_ratio: f32 = 0.2, NONE;
});

/// How a control tunes its snaps (the overrides of SkiaCarousel and SkiaDrawer).
#[derive(Clone, Copy)]
pub(crate) struct Tuning {
    /// Multiplies the velocity a snap starts with (SkiaCarousel: SwipeSpeed / 2).
    pub velocity_scale: f32,
    /// Multiplies the stiffness of the spring (SkiaCarousel: SwipeSpeed).
    pub stiffness_scale: f32,
    /// Multiplies the velocity the spring starts with, after `velocity_scale` (SkiaCarousel:
    /// SwipeSpeed).
    pub spring_velocity_scale: f32,
    /// The speed of a snap the finger left none to, points per second: `None` =
    /// `auto_velocity_multiply_pts` times the scale (SkiaDrawer: 1500 along its axis).
    pub auto_velocity: Option<f32>,
    pub duration: Duration,
    pub easing: Easing,
}

/// How long an eased (not bouncing) snap takes.
#[derive(Clone, Copy)]
pub(crate) enum Duration {
    /// 0.7 s times 300 / speed within 0.1..0.8 for a speed over 10, else 0.3 s times the way over
    /// the control's height.
    Base,
    /// SkiaCarousel: the time the speed takes over the way, times the way over the slide height,
    /// at most 0.25 s over `swipe_speed / 2`; `linear_speed_ms` per whole slide instead when set.
    /// `cell` is the slide size, points.
    Carousel { swipe_speed: f32, linear_speed_ms: f32, cell: Point, vertical: bool },
}

impl Tuning {
    pub(crate) const BASE: Tuning = Tuning {
        velocity_scale: 1.0,
        stiffness_scale: 1.0,
        spring_velocity_scale: 1.0,
        auto_velocity: None,
        duration: Duration::Base,
        easing: easing::cubic_in_out,
    };
}

/// A snap on its way: a spring per axis, or one eased run from a point to another.
#[derive(Clone, Copy)]
enum Motion {
    Spring { x: Option<Bounce>, y: Option<Bounce> },
    Eased { from: Point, to: Point, ms: f32, easing: Easing },
}

/// The position of a snapping control and how it moves. Points.
pub(crate) struct Snapping {
    pub snap_points: Vec<Point>,
    /// Where the content is (DrawnUI CurrentPosition).
    pub position: Point,
    /// The point the content rests on or travels to (CurrentSnap); (-1, -1) before the first.
    pub snap: Point,
    /// How far the content may go without the rubber band (ContentOffsetBounds).
    pub bounds: Rect,
    pub is_user_panning: bool,
    pub is_user_focused: bool,
    /// The content is between snap points, or on its way (InTransition).
    pub in_transition: bool,
    pub accumulator: VelocityAccumulator,
    motion: Option<Motion>,
    /// Frame time of the first frame of the motion.
    start_ms: Option<f64>,
    /// The size of the control, points: the rubber band and the base duration use it.
    pub size: Point,
    pub scale: f32,
}

impl Default for Snapping {
    fn default() -> Self {
        Self {
            snap_points: Vec::new(),
            position: Point::default(),
            snap: Point::new(-1.0, -1.0),
            bounds: Rect::default(),
            is_user_panning: false,
            is_user_focused: false,
            in_transition: false,
            accumulator: VelocityAccumulator::default(),
            motion: None,
            start_ms: None,
            size: Point::default(),
            scale: 1.0,
        }
    }
}

pub(crate) fn dist(a: Point, b: Point) -> f32 {
    (a - b).length()
}

/// Within a point of each other on both axes.
pub(crate) fn near(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() <= 1.0 && (a.y - b.y).abs() <= 1.0
}

impl Snapping {
    pub fn is_animating(&self) -> bool {
        self.motion.is_some()
    }

    /// Stops a snap where it is, without counting as a finished one (React StopSnapAnimators).
    pub fn stop(&mut self) {
        self.motion = None;
    }

    /// The extent covered by the snap points (React BoundsFromSnapPoints).
    pub fn bounds_from_snap_points(&self) -> Rect {
        let Some(first) = self.snap_points.first() else { return Rect::default() };
        self.snap_points.iter().fold(Rect::new(first.x, first.y, first.x, first.y), |b, p| {
            Rect::new(b.left.min(p.x), b.top.min(p.y), b.right.max(p.x), b.bottom.max(p.y))
        })
    }

    /// Inside the bounds; `rubber`: past them by the rubber band over the control's size instead
    /// of stopping there (React ClampOffset, RubberBandUtils.ClampOnTrack).
    pub fn clamp(&self, x: f32, y: f32, rubber: bool, rubber_effect: f32) -> Point {
        let b = self.bounds;
        let hard = Point::new(x.clamp(b.left, b.right), y.clamp(b.top, b.bottom));
        if !rubber {
            return hard;
        }
        let band = |value: f32, hard: f32, dim: f32| {
            let over = value - hard;
            let dim = if dim == 0.0 { 40.0 } else { dim };
            if over == 0.0 { hard } else { hard + over.signum() * (1.0 - 1.0 / (over.abs() * rubber_effect / dim + 1.0)) * dim }
        };
        Point::new(band(x, hard.x, self.size.x), band(y, hard.y, self.size.y))
    }

    /// The snap point nearest to `current`, or `current` without any.
    pub fn nearest_anchor(&self, current: Point) -> Point {
        let mut best = (current, f32::INFINITY);
        for &p in &self.snap_points {
            let d = dist(p, current);
            if d < best.1 {
                best = (p, d);
            }
        }
        best.0
    }

    /// The closest snap point that lies the way the velocity points, else `origin` (React
    /// SelectNextAnchor).
    pub fn select_next_anchor(&self, origin: Point, velocity: Point) -> Point {
        let len = velocity.length();
        if len == 0.0 {
            return origin;
        }
        let direction = velocity * (1.0 / len);
        // ponytail: sorts a copy of the points per release; carousels have few.
        let mut ordered = self.snap_points.clone();
        ordered.sort_by(|a, b| dist(*a, origin).total_cmp(&dist(*b, origin)));
        for anchor in ordered {
            let (way, l) = (anchor - origin, dist(anchor, origin));
            if l != 0.0 && direction.x * (way.x / l) + direction.y * (way.y / l) > 0.0 {
                return anchor;
            }
        }
        origin
    }

    fn duration_secs(&self, start: Point, end: Point, velocity: Point, displacement: Point, tuning: &Tuning) -> f32 {
        match tuning.duration {
            Duration::Base => {
                let magnitude = velocity.length();
                if magnitude > 10.0 {
                    return 0.7 * (300.0 / magnitude).clamp(0.1, 0.8);
                }
                let height = if self.size.y == 0.0 { 1.0 } else { self.size.y };
                0.3 * (displacement.x.abs().max(displacement.y.abs()) / height)
            }
            Duration::Carousel { swipe_speed, linear_speed_ms, cell, vertical } => {
                let max = 0.25 / (swipe_speed / 2.0);
                // As C#: both axes over the height.
                let height = if cell.y == 0.0 { 1.0 } else { cell.y };
                let (velocity, displacement, way) = match vertical {
                    true => (velocity.y, displacement.y, (end.y - start.y).abs()),
                    false => (velocity.x, displacement.x, (end.x - start.x).abs()),
                };
                let mut speed = if velocity != 0.0 { ((displacement / velocity).abs() * (way / height)).min(max) } else { max };
                let slide = if vertical { cell.y } else { cell.x };
                if linear_speed_ms > 0.0 && slide > 0.0 {
                    speed = (way / slide) * (linear_speed_ms / 1000.0);
                }
                speed
            }
        }
    }

    /// React ScrollToOffset: goes to `target`, by a spring with `bounces`, else eased over the
    /// duration the velocity gives; `animate` off: jumps. False when `target` is the snap already.
    pub fn scroll_to_offset(&mut self, target: Point, mut velocity: Point, animate: bool, props: &SnappingProps, tuning: &Tuning) -> bool {
        if target == self.snap {
            return false;
        }
        self.stop();
        if animate && self.size.y > 0.0 {
            let start = self.position;
            let displacement = start - target;
            if velocity.is_zero() {
                let v = tuning.auto_velocity.unwrap_or(props.auto_velocity_multiply_pts * self.scale);
                velocity = Point::new(-v * sign(displacement.x), -v * sign(displacement.y));
            }
            velocity *= tuning.velocity_scale;
            if !displacement.is_zero() {
                self.in_transition = true;
                self.motion = Some(if props.bounces {
                    // ponytail: the underdamped spring only; a `rubber_damping` of 1 and more (React:
                    // critically damped, or an error) is taken just below it.
                    let (mass, ratio) = (1.0 + props.rubber_damping, (0.5 * (1.0 + props.rubber_damping)).min(0.999));
                    let stiffness = 200.0 * tuning.stiffness_scale;
                    let v = velocity * tuning.spring_velocity_scale;
                    let spring = |displacement: f32, velocity: f32, to: f32| {
                        (displacement != 0.0).then(|| Bounce::with_spring(to, displacement, velocity, mass, stiffness, ratio))
                    };
                    Motion::Spring { x: spring(displacement.x, v.x, target.x), y: spring(displacement.y, v.y, target.y) }
                } else {
                    let ms = (self.duration_secs(start, target, velocity, displacement, tuning) * 1000.0).max(60.0);
                    Motion::Eased { from: start, to: target, ms, easing: tuning.easing }
                });
                self.start_ms = None;
            }
        } else {
            self.position = target;
        }
        self.snap = target;
        true
    }

    /// One frame of the snap. The new position (also set), and whether the snap ended in this
    /// frame; `None` when nothing moves. The first frame of a motion is its start (React: an
    /// animator's first tick is at 0 s).
    pub fn animate(&mut self, time_ms: f64) -> Option<(Point, bool)> {
        let motion = self.motion?;
        let start = *self.start_ms.get_or_insert(time_ms);
        let secs = ((time_ms - start) / 1000.0) as f32;
        let (position, done) = match motion {
            Motion::Spring { x, y } => {
                let (px, dx) = x.map_or((self.position.x, true), |x| x.update(secs));
                let (py, dy) = y.map_or((self.position.y, true), |y| y.update(secs));
                (Point::new(px, py), dx && dy)
            }
            Motion::Eased { from, to, ms, easing } => {
                let progress = secs * 1000.0 / ms;
                if progress >= 1.0 { (to, true) } else { (from + (to - from) * easing(progress), false) }
            }
        };
        self.position = position;
        if done {
            self.motion = None;
        }
        Some((position, done))
    }

    /// A snap started by code between frames (the app set a property): React starts its animator
    /// at once, so its first frame is the next one at 0 s. Here the property is taken during that
    /// next frame, after the animators ran: the snap counts from that frame instead.
    pub fn started_at(&mut self, time_ms: f64) {
        if self.motion.is_some() && self.start_ms.is_none() {
            self.start_ms = Some(time_ms);
        }
    }

    /// React CheckTransitionEnded: at rest within a point of the snap, and nothing on its way.
    pub fn transition_ended(&self) -> bool {
        !self.is_animating() && near(self.position, self.snap)
    }
}

/// JavaScript's Math.sign: 0 stays 0.
pub(crate) fn sign(v: f32) -> f32 {
    if v == 0.0 { 0.0 } else { v.signum() }
}
